//! The operation log's on/off switch (`operationLog.enabled`, the user's
//! opt-out from recording file activity).
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
//! [`OperationLog::journal_open`](super::OperationLog::journal_open) consults
//! this once and fixes the answer in the operation's `OpJournal`, so every
//! journal implementation (production and test) sees the same gate.

use std::sync::atomic::{AtomicBool, Ordering};

/// The switch.
pub(super) struct Recording {
    /// Whether new operations are journaled. On by default; the host seeds it
    /// from settings before opening the journal and pushes changes live.
    enabled: AtomicBool,
}

impl Recording {
    pub(super) fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
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
}

#[cfg(test)]
mod tests;
