//! An engine of a test's own ([`TestOperations`], `engine.rs`), and what test
//! cells outside the engine reach beyond its public list: the internals they
//! drive directly and the fixture they park operations with. A cell outside the
//! engine names the engine it drives, its own or the app's
//! (`operations_host::app_engine()`).
//!
//! Sized for the crate's `testing` feature: when the engine becomes a crate,
//! exactly this list widens from `cfg(test)` to `any(test, feature = "testing")`.
//! Gated items count apart from the public surface's ceilings, so this list is
//! the place a test-only reach goes, ❌ never the public list above it.

use std::path::PathBuf;
use std::time::Duration;

use super::Operations;

// An engine a test builds for itself, over fakes it reads back, and a journal
// it reads rows back from.
mod engine;
pub use engine::{RegisteredOperation, TestOperationLog, TestOperations};

// A real queued operation, parked on the engine a cell names.
pub use super::test_support::QueuedOperationFixture;

// Driving one operation's state and sink by hand.
pub use super::event_sinks::CollectorEventSink;
pub use super::state::{IntentCell, OperationIntent, WriteOperationState};

/// A fresh state on `engine`, unregistered: what a cell outside the engine drives
/// a volume driver with directly (a backend suite, on `operations_host::app_engine()`).
pub fn new_operation_state(engine: &Operations, progress_interval: Duration) -> WriteOperationState {
    engine.new_operation_state(progress_interval)
}

// What a suite asserts on: the settle a mutation reports, the binding a
// reviewed batch applies, the live tree's ancestry, and the index's leaf cap.
pub use super::journal_search::SEARCH_LEAF_CAP;
pub use super::mutation_reply::MutationSettledOutcome;
pub use super::source_binding::retain_bound_sources;
pub use super::transfer_tree::TransferTreeAncestor;

// Wire vocabulary only a payload a test builds by hand, or an assertion
// matches field by field, needs.
pub use super::types::{
    AppearedDuringMove, CancelRollback, LostMetadata, LostMetadataClass, MetadataLeftBehind, ProgressAxis,
    SourcesGoneNext, TransferWaitReason,
};

// The backend suites (`operations_host/backend_suites/`): the volume engine's
// drivers a cell runs on a live server's volume with its own state and sink,
// and the fault injector and safety oracle its scenarios wrap around them.
pub use super::transfer::volume::{
    FaultyOp, FaultyVolume, SafetySpec, assert_operation_was_safe, copy_volumes_with_progress,
    move_volumes_with_progress, move_within_same_volume_with_progress,
};
pub use crate::forward_volume_methods;

// The same suites' other direct drivers: a volume delete, a mkdir, a zip
// edit's remote steps, and an outage policy.
pub use super::archive_edit::{EditError, pull_apply_upload_swap};
pub use super::create::create_directory_core_on;

// The cells that drive the engine over the app's registry
// (`operations_host/registry_routing_tests.rs`): a zip edit's request, and the
// duplicate pre-check inside an archive.
pub use super::archive_edit::{ArchiveEditRequest, archive_inner_exists};
pub use super::manager::OperationSummaryText;

// The scan preview's log label, which the app's report redactor must clean
// (`operations_host/log_redaction_tests.rs`).
pub use super::delete::delete_volume_files_for_test;
pub use super::outage_slot::OutagePolicy;
pub use super::scan_watchdog::ScanTarget;

/// Seeds `engine`'s previews with an entry that fails the coherence canary, for a
/// backend cell pinning what an operation does when a half-empty preview reaches
/// it anyway. See `ScanPreviews::seed_incoherent_scan_result_for_test`.
pub fn seed_incoherent_scan_result(
    engine: &Operations,
    preview_id: String,
    sources: Vec<PathBuf>,
    file_count: usize,
    total_bytes: u64,
) {
    engine
        .stores()
        .previews()
        .seed_incoherent_scan_result_for_test(preview_id, sources, file_count, total_bytes);
}

// An undo's executor input, which the rollback cells build from a journaled
// operation.
pub use super::rollback::Reversal;
