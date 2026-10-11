//! The live transfer tree: a copy, move, or compress's selection as a tree,
//! with every folder's and file's progress, for the progress dialog and the
//! queue to show which folder is in flight, what landed, and what's next.
//!
//! The backend owns the whole model; a view asks for a window of flattened
//! rows (`view.rs`) and holds nothing but its own hand toggles. Map, budget,
//! and decisions: `CLAUDE.md` and `DETAILS.md` beside this file.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the drivers' reports and the view command arrive in milestone 3 of docs/specs/transfer-tree-progress.md"
    )
)]

mod build;
mod model;
mod order;
mod view;

#[cfg(test)]
#[path = "budget_tests.rs"]
mod budget_tests;
#[cfg(test)]
pub(super) mod test_support;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) use build::{TreeRecorder, from_archive_plan, from_local_scan, from_per_path};
pub(crate) use model::{NodeId, TreeModel};
pub(crate) use order::TreeOrder;
use view::{TransferTreeView, TransferTreeViewRequest};

/// One operation's tree, shared by its drivers (which report into it) and every
/// window viewing it.
///
/// ❗ One lock, taken once per file BOUNDARY (begin, complete, skip), ❌ never
/// per chunk: an active file's bytes live in its driver's own counter
/// (`model::ActiveLeaf`), which a view reads without asking the driver.
pub(crate) struct TransferTree {
    model: Mutex<TreeModel>,
    /// Bumps on every mutation; a view reports it so a caller can tell nothing
    /// changed.
    revision: AtomicU64,
}

impl TransferTree {
    pub(crate) fn new(model: TreeModel) -> Self {
        Self {
            model: Mutex::new(model),
            revision: AtomicU64::new(0),
        }
    }

    /// Applies one file-boundary report.
    pub(crate) fn mutate<R>(&self, apply: impl FnOnce(&mut TreeModel) -> R) -> R {
        let result = apply(&mut self.lock());
        self.revision.fetch_add(1, Ordering::Relaxed);
        result
    }

    /// Reads the model under the lock.
    pub(crate) fn read<R>(&self, read: impl FnOnce(&TreeModel) -> R) -> R {
        read(&self.lock())
    }

    /// A window of flattened rows (`view.rs`).
    pub(crate) fn view(&self, request: &TransferTreeViewRequest) -> TransferTreeView {
        let model = self.lock();
        let revision = self.revision.load(Ordering::Relaxed);
        view::flatten(&model, request).with_revision(revision)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TreeModel> {
        // A poisoned tree is a display model: what it held is still the best
        // picture of the transfer, and the transfer itself must not stop.
        use crate::ignore_poison::IgnorePoison;
        self.model.lock_ignore_poison()
    }
}

/// Where an operation keeps its tree (`WriteOperationState::transfer_tree`):
/// empty while the scan runs, set once after.
// DEFAULT-OK: an empty slot is exactly "this operation has no tree yet".
#[derive(Default)]
pub(crate) struct TransferTreeSlot(OnceLock<Arc<TransferTree>>);

impl TransferTreeSlot {
    /// Hands the operation the tree its scan built. Once: a later call (no
    /// operation scans twice today) keeps the first tree, whose ids views may
    /// already hold.
    pub(crate) fn install(&self, model: TreeModel) {
        let nodes = model.len();
        let kib = model.heap_bytes() / 1024;
        if self.0.set(Arc::new(TransferTree::new(model))).is_ok() {
            // allowed-pluralize-noun: a debug log line, never shown to anyone
            log::debug!(target: "cmdr_lib::write_operations", "transfer tree: {nodes} nodes in {kib} KiB");
        }
    }

    /// The tree, once the scan has built one.
    pub(crate) fn get(&self) -> Option<&Arc<TransferTree>> {
        self.0.get()
    }
}
