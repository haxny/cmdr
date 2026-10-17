//! What test cells outside the engine reach beyond its public list: the
//! internals they drive directly, the fixtures they park operations with, and
//! the shared engine they run on until each test builds its own.
//!
//! Sized for the crate's `testing` feature: when the engine becomes a crate,
//! exactly this list widens from `cfg(test)` to `any(test, feature = "testing")`.
//! Gated items count apart from the public surface's ceilings, so this list is
//! the place a test-only reach goes, ❌ never the public list above it.

use super::{WriteOperationError, WriteOperationType};

// The engine every test shares, and the fixtures that park or register an
// operation on it.
pub(crate) use super::installed;
pub(crate) use super::test_support::QueuedOperationFixture;

// Driving one operation's state and sink by hand.
pub(crate) use super::event_sinks::CollectorEventSink;
pub(crate) use super::state::WriteOperationState;

// What a suite asserts on: the settle a mutation reports, the binding a
// reviewed batch applies, the live tree's ancestry, and the index's leaf cap.
pub(crate) use super::journal_search::SEARCH_LEAF_CAP;
pub(crate) use super::mutation_reply::MutationSettledOutcome;
pub(crate) use super::source_binding::retain_bound_sources;
pub(crate) use super::transfer_tree::TransferTreeAncestor;

// Wire vocabulary only a payload a test builds by hand needs.
pub(crate) use super::types::{CancelRollback, ProgressAxis, SourcesGoneNext, TransferWaitReason};

// The live-SMB suite helpers app-side SMB cells share.
pub(crate) use super::backend_suites::smb_test_support;

/// Retains a failure straight in the shared engine's manager, so a suite
/// outside the engine can exercise a CONSUMER of `list_operations` (the MCP
/// `queue` guard) against a failed row without spawning a real operation. Drop
/// it again with the ordinary `Operations::dismiss_failed_operation`.
pub(crate) fn retain_failure(operation_id: &str, operation_type: WriteOperationType, error: &WriteOperationError) {
    installed().retain_failure(operation_id, operation_type, error);
}
