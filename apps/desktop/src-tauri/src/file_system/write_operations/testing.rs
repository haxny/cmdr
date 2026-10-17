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
pub(crate) use super::test_support::{QueuedOperationFixture, TestOperationGuard};

// Driving one operation's state and sink by hand.
pub(crate) use super::event_sinks::CollectorEventSink;
pub(crate) use super::state::{OperationIntent, WriteOperationState};

// What a suite asserts on: the settle a mutation reports, the binding a
// reviewed batch applies, the live tree's ancestry, and the index's leaf cap.
pub(crate) use super::journal_search::SEARCH_LEAF_CAP;
pub(crate) use super::mutation_reply::MutationSettledOutcome;
pub(crate) use super::source_binding::retain_bound_sources;
pub(crate) use super::transfer_tree::TransferTreeAncestor;

// Wire vocabulary only a payload a test builds by hand, or an assertion
// matches field by field, needs.
pub(crate) use super::types::{
    AppearedDuringMove, CancelRollback, LostMetadata, LostMetadataClass, MetadataLeftBehind, ProgressAxis,
    SourcesGoneNext, TransferWaitReason,
};

// The backend suites (`operations_host/backend_suites/`): the volume engine's
// drivers a cell runs on a live server's volume with its own state and sink,
// the fault injector and safety oracle its scenarios wrap around them, and the
// stall dump a hung cell prints.
pub(crate) use super::transfer::transfer_probe::render_live_dump as render_live_transfer_dump;
pub(crate) use super::transfer::volume::{
    FaultyOp, FaultyVolume, SafetySpec, assert_operation_was_safe, copy_volumes_with_progress, forward_volume_methods,
    move_volumes_with_progress, move_within_same_volume_with_progress,
};

// The same suites' other direct drivers: a volume delete, a mkdir, a zip
// edit through the archive routing, an outage policy, and a preview seeded
// into the state a racing writer leaves.
pub(crate) use super::archive_edit::{EditError, pull_apply_upload_swap, route_archive_copy_into};
pub(crate) use super::create::create_directory_core;
pub(crate) use super::delete::delete_volume_files_for_test;
pub(crate) use super::outage_slot::OutagePolicy;
pub(crate) use super::scan_cache::seed_incoherent_scan_result_for_test;

// An undo's executor input, which the rollback cells build from a journaled
// operation.
pub(crate) use super::rollback::Reversal;

/// Retains a failure straight in the shared engine's manager, so a suite
/// outside the engine can exercise a CONSUMER of `list_operations` (the MCP
/// `queue` guard) against a failed row without spawning a real operation. Drop
/// it again with the ordinary `Operations::dismiss_failed_operation`.
pub(crate) fn retain_failure(operation_id: &str, operation_type: WriteOperationType, error: &WriteOperationError) {
    installed().retain_failure(operation_id, operation_type, error);
}
