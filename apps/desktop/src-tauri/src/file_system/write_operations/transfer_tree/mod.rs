//! The live transfer tree: a copy, move, or compress's selection as a tree,
//! with every folder's and file's progress, for the progress dialog and the
//! queue to show which folder is in flight, what landed, and what's next.
//!
//! The backend owns the whole model; a view asks for a window of flattened
//! rows (`view.rs`) and holds nothing but its own hand toggles. Drivers report
//! through `report.rs`. Map, budget, and decisions: `CLAUDE.md` and
//! `DETAILS.md` beside this file.

mod build;
mod model;
mod order;
mod report;
mod view;

#[cfg(test)]
#[path = "budget_tests.rs"]
mod budget_tests;
#[cfg(test)]
#[path = "report_tests.rs"]
mod report_tests;
#[cfg(test)]
pub(super) mod test_support;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) use build::{TreeRecorder, from_archive_plan, from_local_scan, from_per_path, from_sources, sort_sources};
pub(crate) use model::{NodeId, TreeModel};
pub(crate) use order::TreeOrder;
pub(crate) use report::{TreeLeaf, TreeReporter};
pub(crate) use view::{TransferTreeView, TransferTreeViewRequest};

use super::types::{ProgressAxis, WriteOperationError};

/// One operation's tree, shared by its drivers (which report into it) and every
/// window viewing it.
///
/// ❗ One lock, taken once per file BOUNDARY (begin, complete, skip), ❌ never
/// per chunk: an active file's bytes live in its driver's own counter
/// (`model::ActiveLeaf`), which a view reads without asking the driver.
///
/// ❗ Observation only. A report can't return an error, and a report that
/// panics (a tree bug) is caught here, logged, and turns the tree off for the
/// rest of the operation: the transfer it describes carries on untouched.
pub(crate) struct TransferTree {
    model: Mutex<TreeModel>,
    /// Bumps on every mutation; a view reports it so a caller can tell nothing
    /// changed.
    revision: AtomicU64,
    /// A report or a view panicked. Nothing reads or writes the model after
    /// that: its counts may be half-applied, and a view says "no tree".
    broken: AtomicBool,
}

impl TransferTree {
    pub(crate) fn new(model: TreeModel) -> Self {
        Self {
            model: Mutex::new(model),
            revision: AtomicU64::new(0),
            broken: AtomicBool::new(false),
        }
    }

    /// Applies one file-boundary report. `None` once the tree is off.
    pub(crate) fn mutate<R>(&self, apply: impl FnOnce(&mut TreeModel) -> R) -> Option<R> {
        let result = self.guarded(|| apply(&mut self.lock()))?;
        self.revision.fetch_add(1, Ordering::Relaxed);
        Some(result)
    }

    /// Reads the model under the lock. `None` once the tree is off.
    pub(crate) fn read<R>(&self, read: impl FnOnce(&TreeModel) -> R) -> Option<R> {
        self.guarded(|| read(&self.lock()))
    }

    /// A window of flattened rows (`view.rs`), or `None` once the tree is off.
    pub(crate) fn view(&self, request: &TransferTreeViewRequest, axis: ProgressAxis) -> Option<TransferTreeView> {
        self.guarded(|| {
            let model = self.lock();
            let revision = self.revision.load(Ordering::Relaxed);
            view::flatten(&model, request).finished(revision, axis)
        })
    }

    /// Runs `body` unless the tree is off, and turns it off if `body` panics.
    /// The panic still reaches the crash reporter's hook (it's a bug); only the
    /// unwinding stops here, so it never reaches the driver that reported.
    fn guarded<R>(&self, body: impl FnOnce() -> R) -> Option<R> {
        if self.broken.load(Ordering::Relaxed) {
            return None;
        }
        match catch_unwind(AssertUnwindSafe(body)) {
            Ok(result) => Some(result),
            Err(_) => {
                self.broken.store(true, Ordering::Relaxed);
                crate::log_error!(
                    "transfer tree: a report panicked, so this operation shows no tree from here on (the transfer carries on)"
                );
                None
            }
        }
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

    /// The tree, once the scan has built one. Drivers go through
    /// [`reporter`](Self::reporter) and views through [`view`](Self::view).
    #[cfg(test)]
    pub(crate) fn get(&self) -> Option<&Arc<TransferTree>> {
        self.0.get()
    }

    /// What a driver reports through: the tree, or nothing to report to.
    pub(crate) fn reporter(&self) -> TreeReporter {
        TreeReporter::new(self.0.get().cloned())
    }

    /// Marks the row a failure names, from the typed path on the error
    /// (`WriteOperationError::item_path`), ❌ never its message. A path the
    /// tree doesn't hold (a destination path, an operation-wide refusal)
    /// marks nothing.
    pub(crate) fn note_failure(&self, error: &WriteOperationError) {
        self.reporter().fail_at(error);
    }

    /// The window a view asked for. `ready: false` while the scan runs, and
    /// for a tree a bug turned off.
    pub(crate) fn view(&self, request: &TransferTreeViewRequest, axis: ProgressAxis) -> TransferTreeView {
        self.0
            .get()
            .and_then(|tree| tree.view(request, axis))
            .unwrap_or_else(|| TransferTreeView::not_ready(axis))
    }
}
