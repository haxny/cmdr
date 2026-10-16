//! The operation log's on/off switch (`operationLog.enabled`, the user's
//! opt-out from recording file activity), and the set of operations that opened
//! with nothing to journal to.
//!
//! Off means NEW operations aren't journaled, so they can't be undone. Everything
//! else keeps working: the journal stays installed, history stays readable,
//! retention keeps pruning it, and an operation already in the log can still be
//! rolled back (the rollback engine writes through the writer, not this gate).
//!
//! **The decision is made once per operation, at open, and holds to finalize.**
//! An operation that opened while recording was on journals every item even if
//! the switch flips mid-way; one that opened while it was off journals nothing,
//! even if it flips back on. A half-journaled operation would be worse than
//! either: its completeness check can't see rows that were never issued, so it
//! could claim an undo that reverses only part of what happened. The same holds
//! for an operation that opened before the journal did: it stays unjournaled
//! when the journal arrives mid-way.
//!
//! [`OperationLog`](super::OperationLog) consults this before reaching the
//! journal, so every journal implementation (production and test) sees the same
//! gate.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;

/// The switch plus the operations it (or a missing journal) ruled out at open.
pub(super) struct Recording {
    /// Whether new operations are journaled. On by default; the host seeds it
    /// from settings before opening the journal and pushes changes live.
    enabled: AtomicBool,
    /// Operations that opened with nothing to journal to (recording off, or no
    /// journal yet), until they finalize. Their record calls are dropped here,
    /// so a journal never sees an op it didn't open.
    unrecorded: Mutex<BTreeSet<String>>,
    /// `unrecorded.len()`, readable without the lock: the per-batch record path
    /// checks it first, so a journaled op's record call never takes the lock.
    unrecorded_len: AtomicUsize,
}

impl Recording {
    pub(super) fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            unrecorded: Mutex::new(BTreeSet::new()),
            unrecorded_len: AtomicUsize::new(0),
        }
    }

    /// Turn recording of new operations on or off. Operations already open keep
    /// the decision they were opened with.
    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Whether new operations are journaled right now.
    pub(super) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Remember an operation that opened with nothing to journal to, until
    /// [`Self::release`].
    pub(super) fn hold_unrecorded(&self, op_id: &str) {
        let mut unrecorded = self.unrecorded.lock_ignore_poison();
        if unrecorded.insert(op_id.to_string()) {
            self.unrecorded_len.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Whether this operation opened with nothing to journal to.
    pub(super) fn is_unrecorded(&self, op_id: &str) -> bool {
        self.unrecorded_len.load(Ordering::SeqCst) > 0 && self.unrecorded.lock_ignore_poison().contains(op_id)
    }

    /// Forget an operation at finalize. Returns whether it was unrecorded, so the
    /// caller skips the journal's finalize too.
    pub(super) fn release(&self, op_id: &str) -> bool {
        if self.unrecorded_len.load(Ordering::SeqCst) == 0 {
            return false;
        }
        let removed = self.unrecorded.lock_ignore_poison().remove(op_id);
        if removed {
            self.unrecorded_len.fetch_sub(1, Ordering::SeqCst);
        }
        removed
    }

    /// Back to the default (on, nothing unrecorded). The app's shared-handle test
    /// guard calls this on both ends, so a test that switches recording off can't
    /// leak it into the next.
    #[cfg(any(test, feature = "testing"))]
    pub(super) fn reset(&self) {
        self.set_enabled(true);
        self.unrecorded.lock_ignore_poison().clear();
        self.unrecorded_len.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests;
