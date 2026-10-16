//! `OperationLog`: the operation log's state as a value the host builds and
//! holds, instead of process statics.
//!
//! It owns the journal slot (empty until [`OperationLog::open`] fills it), the
//! recording gate, and the writer once opened. A handle starts detached, so an
//! operation opened before the journal journals nothing, and the host opens it
//! once its data dir is known. Two handles share nothing: a test builds its own.
//! `DETAILS.md` § "The seam: a journal reached by `op_id`".

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::ignore_poison::RwLockIgnorePoison;

use super::capture::{FinalizeInputs, OperationJournal, WriterJournal};
use super::recording::Recording;
use super::retention::{self, RetentionLimitsSource};
use super::rollback::reconcile_rolling_back_on_open;
use super::store::{OperationLogStore, OperationLogStoreError};
use super::types::{NotRollbackableReason, SearchCoverage, SearchCoverageReason};
use super::writer::{FinalizeOutcome, JournalItem, OpenOperation, OperationLogWriter};

/// The operation log: a cheap-to-clone handle, every clone sharing one state.
#[derive(Clone)]
pub struct OperationLog {
    inner: Arc<Inner>,
}

struct Inner {
    /// The journal, and the writer behind it, published together in ONE write.
    slot: RwLock<Option<Published>>,
    recording: Recording,
    /// Set by the first [`OperationLog::open`], whatever its outcome.
    open_claimed: AtomicBool,
}

#[derive(Clone)]
struct Published {
    journal: Arc<dyn OperationJournal>,
    /// `None` for a journal a test or bench installed by hand.
    writer: Option<OperationLogWriter>,
}

/// Why [`OperationLog::open`] left the handle without a journal.
#[derive(Debug)]
pub enum OpenError {
    /// This handle already ran `open`; there's one writer per handle.
    AlreadyOpen,
    /// The store couldn't open the DB (migrate, recreate, or a refused downgrade).
    Store(OperationLogStoreError),
    /// The writer thread couldn't open its connection or start.
    Writer(OperationLogStoreError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::AlreadyOpen => write!(f, "the operation log is already open"),
            OpenError::Store(e) => write!(f, "the operation log store didn't open: {e}"),
            OpenError::Writer(e) => write!(f, "the operation log writer didn't start: {e}"),
        }
    }
}

impl std::error::Error for OpenError {}

impl OperationLog {
    /// An empty handle: recording on, no journal. It journals nothing until
    /// [`Self::open`].
    pub fn detached() -> Self {
        Self {
            inner: Arc::new(Inner {
                slot: RwLock::new(None),
                recording: Recording::new(),
                open_claimed: AtomicBool::new(false),
            }),
        }
    }

    /// Open `db_path` and start journaling into it: the store migrates the schema
    /// forward (or recreates an unparseable file, or refuses a downgrade), the
    /// writer thread starts, the operations a crash left mid-rollback resolve,
    /// and only THEN do the writer and the journal become reachable, together.
    /// So a rollback can never be dispatched into a reconcile.
    ///
    /// Once per handle: a second call is [`OpenError::AlreadyOpen`] and touches
    /// nothing, and a failed open leaves the handle without a journal for good,
    /// which the app survives (it runs without history rather than refusing to
    /// start).
    pub fn open(&self, db_path: &Path) -> Result<(), OpenError> {
        if self.inner.open_claimed.swap(true, Ordering::SeqCst) {
            return Err(OpenError::AlreadyOpen);
        }
        // The store owns the schema lifecycle; the writer then opens its own
        // write connection over the now-current schema.
        OperationLogStore::open(db_path).map_err(OpenError::Store)?;
        let writer = OperationLogWriter::spawn(db_path).map_err(OpenError::Writer)?;
        // Resolve any operation a crash left mid-rollback: from its unfinalized
        // inverse op's recorded outcomes, or straight back to rollbackable when no
        // inverse ever opened. Before anything can journal, so a re-issued
        // rollback resumes cleanly.
        reconcile_rolling_back_on_open(&writer);
        *self.inner.slot.write_ignore_poison() = Some(Published {
            journal: Arc::new(WriterJournal::new(writer.clone())),
            writer: Some(writer),
        });
        log::debug!(target: "operation_log", "operation log ready at {}", db_path.display());
        Ok(())
    }

    /// Turn recording of new operations on or off. Operations already open keep
    /// the decision they were opened with.
    pub fn set_recording(&self, enabled: bool) {
        self.inner.recording.set_enabled(enabled);
    }

    /// The writer, once [`Self::open`] succeeded: for rollback, which journals its
    /// inverse op through it directly, and for the read side's DB path.
    pub fn writer(&self) -> Option<OperationLogWriter> {
        self.inner.slot.read_ignore_poison().as_ref()?.writer.clone()
    }

    /// Prune by age and size now and on a slow timer, on `runtime`, with the
    /// limits `limits` reads fresh each tick. Does nothing until [`Self::open`]
    /// succeeded.
    pub fn spawn_retention(&self, runtime: &tokio::runtime::Handle, limits: RetentionLimitsSource) {
        if let Some(writer) = self.writer() {
            retention::spawn(runtime, writer, limits);
        }
    }

    /// Open an operation's journal. Decides ONCE whether the operation is
    /// journaled: with recording off, or no journal yet, it journals nothing for
    /// its whole life, even if either changes before it finalizes.
    pub fn journal_open(&self, open: OpenOperation) -> OpJournal {
        let journal = if self.inner.recording.is_enabled() {
            self.current_journal()
        } else {
            None
        };
        let op_id = open.op_id.clone();
        match &journal {
            Some(journal) => journal.open(open),
            None => self.inner.recording.hold_unrecorded(&op_id),
        }
        OpJournal {
            log: self.clone(),
            op_id,
            journal,
        }
    }

    // shim: removed in M6a
    /// [`OpJournal::record_items`] by `op_id`, for a caller that doesn't carry
    /// its operation's [`OpJournal`].
    pub fn record_items(&self, op_id: &str, items: Vec<JournalItem>) {
        if let Some(journal) = self.journal_of(op_id) {
            journal.record_items(op_id, items);
        }
    }

    // shim: removed in M6a
    /// [`OpJournal::note_coverage`] by `op_id`.
    pub fn note_coverage(&self, op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
        if let Some(journal) = self.journal_of(op_id) {
            journal.note_search_coverage(op_id, coverage, reason);
        }
    }

    // shim: removed in M6a
    /// [`OpJournal::note_not_rollbackable`] by `op_id`.
    pub fn note_not_rollbackable(&self, op_id: &str, reason: NotRollbackableReason) {
        if let Some(journal) = self.journal_of(op_id) {
            journal.note_not_rollbackable(op_id, reason);
        }
    }

    // shim: removed in M6a
    /// [`OpJournal::finalize`] by `op_id`.
    pub fn finalize(&self, op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome {
        // Released first, so an unrecorded op is forgotten even with no journal.
        let unrecorded = self.inner.recording.release(op_id);
        match self.current_journal() {
            Some(journal) if !unrecorded => journal.finalize(op_id, inputs),
            _ => NOTHING_WRITTEN,
        }
    }

    // shim: removed in M6a
    /// The journal an operation journals to: none if it opened without one.
    fn journal_of(&self, op_id: &str) -> Option<Arc<dyn OperationJournal>> {
        if self.inner.recording.is_unrecorded(op_id) {
            return None;
        }
        self.current_journal()
    }

    fn current_journal(&self) -> Option<Arc<dyn OperationJournal>> {
        Some(Arc::clone(&self.inner.slot.read_ignore_poison().as_ref()?.journal))
    }

    /// Put `journal` in the slot (or empty it) with no writer, bypassing
    /// [`Self::open`]: a test's `CapturingJournal` or temp-DB `WriterJournal`, or
    /// the transfer bench's.
    #[cfg(any(test, feature = "transfer-bench"))]
    pub(crate) fn set_journal(&self, journal: Option<Arc<dyn OperationJournal>>) {
        *self.inner.slot.write_ignore_poison() = journal.map(|journal| Published { journal, writer: None });
    }

    /// Recording back on, nothing unrecorded: a shared handle's test teardown.
    #[cfg(test)]
    pub(crate) fn reset_recording(&self) {
        self.inner.recording.reset();
    }
}

/// What a finalize wrote when nothing was journaled.
const NOTHING_WRITTEN: FinalizeOutcome = FinalizeOutcome {
    rollback_unit_rows: 0,
    search_only_rows: 0,
};

/// One operation's journal, fixed when it opened: the operation is wholly
/// journaled or wholly not. Every method no-ops for an unjournaled operation.
pub struct OpJournal {
    log: OperationLog,
    op_id: String,
    journal: Option<Arc<dyn OperationJournal>>,
}

impl OpJournal {
    /// Record item rows (buffered and batched by the journal).
    pub fn record_items(&self, items: Vec<JournalItem>) {
        if let Some(journal) = &self.journal {
            journal.record_items(&self.op_id, items);
        }
    }

    /// Downgrade the operation's search coverage (worst wins).
    pub fn note_coverage(&self, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
        if let Some(journal) = &self.journal {
            journal.note_search_coverage(&self.op_id, coverage, reason);
        }
    }

    /// Rule the operation out of rollback for a reason only the driver can see
    /// (a directory merge, a conflict resolved after the rows were written).
    /// Beats the per-kind eligibility at finalize.
    pub fn note_not_rollbackable(&self, reason: NotRollbackableReason) {
        if let Some(journal) = &self.journal {
            journal.note_not_rollbackable(&self.op_id, reason);
        }
    }

    /// Finalize the operation, returning the durable per-`row_role` counts. Zero
    /// counts for an unjournaled operation.
    pub fn finalize(self, inputs: FinalizeInputs) -> FinalizeOutcome {
        match &self.journal {
            Some(journal) => journal.finalize(&self.op_id, inputs),
            None => {
                self.log.inner.recording.release(&self.op_id);
                NOTHING_WRITTEN
            }
        }
    }
}

#[cfg(test)]
mod tests;
