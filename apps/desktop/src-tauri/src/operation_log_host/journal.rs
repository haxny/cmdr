//! The write engine's journal calls, by `op_id`, over the app's operation log.
//!
//! The engine reaches the journal by `op_id` at its record points; these keep
//! that shape over [`operation_log()`] until each operation carries its own
//! [`OpJournal`](crate::operation_log::OpJournal). Every call no-ops for an
//! operation that opened with nothing to journal to, and never fails the
//! operation.

// shim: removed in M6a

use crate::operation_log::capture::FinalizeInputs;
use crate::operation_log::types::{NotRollbackableReason, SearchCoverage, SearchCoverageReason};
use crate::operation_log::writer::{FinalizeOutcome, JournalItem, OpenOperation};

use super::operation_log;

/// Open an operation row, deciding once whether the operation is journaled.
pub(crate) fn journal_open(open: OpenOperation) {
    // The by-id calls below find the decision again, so the handle isn't kept.
    drop(operation_log().journal_open(open));
}

/// Record item rows for an open operation (buffered and batched by the journal).
pub(crate) fn journal_record_items(op_id: &str, items: Vec<JournalItem>) {
    operation_log().record_items(op_id, items);
}

/// Downgrade an operation's search coverage (worst wins).
pub(crate) fn journal_note_coverage(op_id: &str, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
    operation_log().note_coverage(op_id, coverage, reason);
}

/// Rule an operation out of rollback for a reason only the driver can see.
pub(crate) fn journal_note_not_rollbackable(op_id: &str, reason: NotRollbackableReason) {
    operation_log().note_not_rollbackable(op_id, reason);
}

/// Finalize an operation, returning the durable per-`row_role` counts.
pub(crate) fn journal_finalize(op_id: &str, inputs: FinalizeInputs) -> FinalizeOutcome {
    operation_log().finalize(op_id, inputs)
}
