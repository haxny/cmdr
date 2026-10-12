//! Shims over the installed engine for `diff_emitter`'s callers (`shims/mod.rs`).

#[cfg(test)]
use crate::file_system::listing::diff::{DiffChange, DirectoryDiffBatch};

#[cfg(any(test, feature = "playwright-e2e"))]
use super::installed;

/// Flushes every listing's pending diffs on the installed engine, now.
// shim: removed in M4b
#[cfg(feature = "playwright-e2e")]
pub(crate) fn flush_all_pending() {
    installed().diffs.flush_all_pending();
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn pending_count(listing_id: &str) -> usize {
    pending_changes_for_test(listing_id).len()
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn flush_now_for_test(listing_id: &str) {
    installed().diffs.flush(listing_id);
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn take_batches_for_test(listing_id: &str) -> Vec<DirectoryDiffBatch> {
    installed().diffs.take_pending(listing_id)
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn hold_for_test(listing_id: &str) {
    installed().diffs.hold(listing_id);
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn pending_batches_for_test(listing_id: &str) -> Vec<DirectoryDiffBatch> {
    installed().diffs.pending_batches(listing_id)
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn pending_changes_for_test(listing_id: &str) -> Vec<DiffChange> {
    pending_batches_for_test(listing_id)
        .into_iter()
        .flat_map(|b| b.changes)
        .collect()
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn enqueue_diff(listing_id: &str, batch: DirectoryDiffBatch) {
    installed().write_cache().publish().enqueue(listing_id, batch);
}

// shim: removed in M4b
#[cfg(test)]
pub(crate) fn drop_pending(listing_id: &str) {
    installed().write_cache().publish().drop_pending(listing_id);
}
