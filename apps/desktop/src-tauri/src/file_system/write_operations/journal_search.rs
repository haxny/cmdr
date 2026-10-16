//! `search_only` leaf enumeration for the operation log.
//!
//! A same-FS move or a trash reverses a whole subtree with ONE rename-back /
//! restore, so the pipeline records ONE top-level `rollback_unit` row (see
//! `journal.rs`). But leaf search ("when did I trash `dog.jpg`") still needs the
//! descendants, so this module enumerates the subtree's leaves from the DRIVE
//! INDEX (zero extra filesystem I/O — the tree is already indexed), through the
//! host's [`host::OperationIndex::search_leaves`], and records them as
//! `search_only` rows beneath the top-level unit.
//!
//! Two hard rules, both load-bearing for honesty:
//!
//! - **Enumerate BEFORE the OS mutation.** The index reconciler prunes the subtree
//!   the moment it sees the trash/rename FSEvent, so a finalize-time read would
//!   find the rows already gone and wrongly stamp `full` over a miss. Callers buffer
//!   the [`BufferedLeaves`] before the syscall.
//! - **Persist only AFTER the item succeeds.** Trash / same-FS move process per
//!   top-level item with partial failure, so persisting at enumeration time would
//!   leave `search_only` leaves for a subtree that was never trashed — and search
//!   has no per-item outcome filter, so "when did I trash `dog.jpg`" would return a
//!   trash that never happened. Callers call [`persist_and_note`] inside the
//!   item's success arm.
//!
//! `search_coverage = full` is gated on the subtree being PRESENT and CURRENT (the
//! index eventually-consistent, so a just-downloaded-then-trashed file can enumerate
//! a stale set) AND the volume index being `Live`; otherwise it downgrades to
//! `top_level_only` with a typed reason so the honest gap stays distinguishable.
//! The host's index answer applies those gates (Cmdr's: `operations_host::index`).

use std::path::Path;

use super::host;
pub(super) use super::host::BufferedLeaves;
#[cfg(test)]
pub(super) use super::host::Leaf;
use cmdr_operation_log::types::SearchCoverage;

/// The per-operation cap on `search_only` leaves enumerated for one top-level
/// trash / same-FS-move item. Because the enumeration is synchronous BEFORE a
/// sub-second rename, a 1M-file folder would otherwise pay a 1M-row index read
/// before the mutation — disproportionate. Over the cap, the op records the
/// top-level `rollback_unit` row only and downgrades coverage to `capped`;
/// rollback is unaffected (the top-level row is the undo unit regardless).
///
/// Initial value benchmark-tuned (see `docs/notes/operation-log-capture-bench.md`).
/// Cheap to change: it only bounds search completeness, never correctness.
pub(crate) const SEARCH_LEAF_CAP: usize = 50_000;

/// Enumerate a subtree's `search_only` leaves through the host's drive index
/// ([`host::OperationIndex::search_leaves`], which applies the Live gate, path
/// resolution, the epoch/coverage gate, and the leaf cap). Called BEFORE the OS
/// mutation; the caller persists the result only after the item succeeds.
/// `volume_id` is the index volume (the local record points pass `"root"`);
/// `abs_source` is the subtree root's absolute path.
pub(super) fn enumerate_subtree_for_search(
    index: &dyn host::OperationIndex,
    volume_id: &str,
    abs_source: &Path,
    cap: usize,
) -> BufferedLeaves {
    #[cfg(test)]
    if let Some(canned) = test_hook::take(abs_source) {
        return canned;
    }
    index.search_leaves(volume_id, abs_source, cap)
}

/// Persist the buffered `search_only` leaves (ONLY call this after the top-level
/// item's mutation succeeded) and note any coverage downgrade on the op. Each leaf
/// is rebased onto the source root (`abs_source`) and, when known, the dest root
/// (`top_level_dest` — the in-trash location, or the moved-to path); a `None` dest
/// records the leaf source-only (still searchable). `source_volume_id` /
/// `dest_volume_id` carry the real volume ids so a volume move's leaves journal
/// under that volume, not `"root"` (the local callers pass `"root"`). A `Full`
/// verdict notes nothing (the op opens `Full`); a downgrade notes worst-wins.
pub(super) fn persist_and_note(
    scope: &super::scope::OpScope,
    source_volume_id: &str,
    abs_source: &Path,
    dest_volume_id: &str,
    top_level_dest: Option<&Path>,
    buffered: &BufferedLeaves,
) {
    for leaf in &buffered.leaves {
        let source = abs_source.join(&leaf.rel);
        let dest = top_level_dest.map(|d| d.join(&leaf.rel));
        super::journal::record_search_leaf(
            scope,
            leaf.entry_type,
            source_volume_id,
            &source,
            dest.as_ref().map(|d| (dest_volume_id, d.as_path())),
            leaf.size,
            leaf.mtime,
        );
    }
    if buffered.coverage != SearchCoverage::Full {
        super::journal::note_coverage(scope, buffered.coverage, buffered.reason);
    }
}

/// Test seam: a per-path canned [`BufferedLeaves`], so the trash / same-FS-move
/// wiring can be exercised (persist-after-success, the failed-item honesty, the
/// capped-but-still-rollbackable case) without standing up a live drive index +
/// registry. Production never installs a hook.
#[cfg(test)]
pub(super) mod test_hook {
    use super::BufferedLeaves;
    use std::cell::RefCell;
    use std::path::Path;

    type Hook = Box<dyn FnMut(&Path) -> Option<BufferedLeaves>>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// Install a hook mapping a subtree-root path to a canned enumeration result
    /// (`None` ⇒ fall through to the real index path for that call).
    #[allow(dead_code, reason = "used only by the macOS-gated trash/move wiring tests")]
    pub(crate) fn install(hook: impl FnMut(&Path) -> Option<BufferedLeaves> + 'static) {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
    }

    #[allow(dead_code, reason = "used only by the macOS-gated trash/move wiring tests")]
    pub(crate) fn clear() {
        HOOK.with(|h| *h.borrow_mut() = None);
    }

    pub(super) fn take(path: &Path) -> Option<BufferedLeaves> {
        HOOK.with(|h| h.borrow_mut().as_mut().and_then(|hook| hook(path)))
    }
}
