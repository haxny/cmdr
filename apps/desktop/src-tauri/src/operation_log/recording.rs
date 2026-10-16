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
//! could claim an undo that reverses only part of what happened.
//!
//! The free functions in [`super`] consult this before reaching the journal, so
//! every journal implementation (production and test) sees the same gate.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::ignore_poison::IgnorePoison;

/// Whether new operations are journaled. On by default; seeded from settings at
/// `start` and pushed live by `set_operation_log_enabled`.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Operations that opened while recording was off, until they finalize. Their
/// record calls are dropped here, so the journal never sees an op it didn't open.
static UNRECORDED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// `UNRECORDED.len()`, readable without the lock: the per-batch record path
/// checks it first, so with recording on it never takes the lock.
static UNRECORDED_LEN: AtomicUsize = AtomicUsize::new(0);

/// Turn recording of new operations on or off. Operations already open keep the
/// decision they were opened with.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

/// Whether new operations are journaled right now.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Decide at open whether this operation is journaled. `false` remembers it as
/// unrecorded until [`release`].
pub(super) fn admit(op_id: &str) -> bool {
    if is_enabled() {
        return true;
    }
    let mut unrecorded = UNRECORDED.lock_ignore_poison();
    if unrecorded.insert(op_id.to_string()) {
        UNRECORDED_LEN.fetch_add(1, Ordering::SeqCst);
    }
    false
}

/// Whether this operation opened while recording was off.
pub(super) fn is_unrecorded(op_id: &str) -> bool {
    UNRECORDED_LEN.load(Ordering::SeqCst) > 0 && UNRECORDED.lock_ignore_poison().contains(op_id)
}

/// Forget an operation at finalize. Returns whether it was unrecorded, so the
/// caller skips the journal's finalize too.
pub(super) fn release(op_id: &str) -> bool {
    if UNRECORDED_LEN.load(Ordering::SeqCst) == 0 {
        return false;
    }
    let removed = UNRECORDED.lock_ignore_poison().remove(op_id);
    if removed {
        UNRECORDED_LEN.fetch_sub(1, Ordering::SeqCst);
    }
    removed
}

/// Back to the default (on, nothing unrecorded). `TestJournalGuard` calls this on
/// both ends, so a test that switches recording off can't leak it into the next.
#[cfg(test)]
pub(super) fn reset_for_test() {
    set_enabled(true);
    UNRECORDED.lock_ignore_poison().clear();
    UNRECORDED_LEN.store(0, Ordering::SeqCst);
}

#[cfg(test)]
mod tests;
