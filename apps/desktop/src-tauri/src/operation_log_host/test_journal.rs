//! Test isolation for the app's shared operation log.

use std::sync::Arc;

use crate::ignore_poison::IgnorePoison;
use crate::operation_log::capture::OperationJournal;

use super::operation_log;

/// Holds the shared [`operation_log()`]'s journal slot for one test.
///
/// The write engine reaches the shared handle by `op_id`, so a test that wants
/// its operations journaled installs a journal there. nextest forks a process per
/// test, so the handle is hermetic there, but plain `cargo test` runs the whole
/// crate's tests as threads in ONE process, where concurrent installs and
/// teardowns race: one test's teardown clears another's journal mid-operation,
/// and rows land in a foreign test's DB. The slot is a single value, not a keyed
/// map, so tests that install a journal SERIALIZE on this guard's lock for their
/// whole duration instead. A test of the operation log itself builds its own
/// `OperationLog` and needs none of this.
///
/// `Drop` clears the slot and resets recording (it runs on unwind too), so a
/// panicking test can't leak its journal into the next. Hold exactly ONE guard
/// per test, on the stack, for the test's whole body; the lock is not reentrant,
/// so constructing a second guard in the same test deadlocks under plain
/// `cargo test`. Multi-arm tests that swap journals mid-test start from
/// [`TestJournalGuard::hold_empty`] and use [`TestJournalGuard::set`] /
/// [`TestJournalGuard::clear`].
pub(crate) struct TestJournalGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestJournalGuard {
    /// Lock the slot and install `journal` for this test's duration.
    pub(crate) fn install(journal: Arc<dyn OperationJournal>) -> Self {
        let guard = Self::hold_empty();
        guard.set(journal);
        guard
    }

    /// Lock the slot and leave it EMPTY: a no-journal baseline no concurrent test
    /// can pollute. Install later with [`Self::set`].
    pub(crate) fn hold_empty() -> Self {
        static JOURNAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        // A panicking journal test poisons the lock, but its `Drop` already cleared
        // the slot, so the next holder starts from a clean baseline regardless.
        let lock = JOURNAL_TEST_LOCK.lock_ignore_poison();
        reset();
        Self { _lock: lock }
    }

    /// Swap the installed journal while keeping the lock (multi-arm tests).
    pub(crate) fn set(&self, journal: Arc<dyn OperationJournal>) {
        operation_log().set_journal(Some(journal));
    }

    /// Drop the installed journal early while keeping the lock (a subsequent arm
    /// runs journal-less, or the test wants the writer joined before reading).
    pub(crate) fn clear(&self) {
        operation_log().set_journal(None);
    }
}

impl Drop for TestJournalGuard {
    fn drop(&mut self) {
        reset();
    }
}

fn reset() {
    operation_log().set_journal(None);
    operation_log().reset_recording();
}
