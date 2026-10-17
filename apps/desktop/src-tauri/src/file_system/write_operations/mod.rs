//! Write operations (copy, move, delete) with streaming progress.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.
//!
//! Safety features:
//! - Path canonicalization to prevent ".." and symlink bypass of recursion checks
//! - Destination writability check before starting
//! - Pre-flight disk space validation after scan
//! - Inode identity check to prevent copy-over-self via symlinks/hard links
//! - Path/name length validation (255-byte name, 1024-byte path)
//! - Special file filtering (skips sockets, FIFOs, devices)
//! - macOS copyfile(3) for full metadata preservation (xattrs, ACLs, resource forks)
//! - Symlink preservation (not dereferenced)
//! - Symlink loop detection to prevent infinite recursion
//! - Atomic cross-filesystem moves using staging directory

mod archive_edit;
mod cancellable;
mod compress_estimate;
mod conflict;
mod conflict_preflight;
mod conflict_slot;
mod create;
mod delete;
mod durability;
mod error_classification;
mod eta;
mod event_sinks;
mod free_space;
mod handle;
// Two engines share nothing.
#[cfg(test)]
mod handle_tests;
pub(crate) mod host;
mod human_wait;
mod in_flight_temps;
mod journal;
mod journal_search;
// The local-filesystem copy, move, delete, and trash starters.
mod ledger;
mod local_start;
mod look_alike;
#[cfg(test)]
mod look_alike_instant_tests;
mod manager;
mod metadata_losses;
mod mutation_error;
mod mutation_reply;
mod operation_intent;
mod outage_slot;
mod overwrite;
// The engine half of "paste clipboard content as a file"; its only caller is macOS.
#[cfg(target_os = "macos")]
mod create_numbered;
mod queued_sources;
mod rename;
mod reversal;
mod rollback;
mod routing;
mod scan;
mod scan_bridge;
mod scan_cache;
mod scan_dry_run;
mod scan_preview;
mod scan_source_tracker;
mod scan_walker;
mod scan_watchdog;
mod scope;
mod scratch_dir;
mod source_binding;
mod sources_gone_slot;
mod state;
mod status_cache;
mod summary;
mod target_names;
mod transfer;
// The Criterion bench's one door into the engine, re-exported at the crate root.
#[cfg(feature = "transfer-bench")]
pub mod transfer_bench;
mod transfer_sides;
mod transfer_tree;
mod tree_view;
mod types;
mod unblocked_removal;
mod unique_name;
mod validation;

use std::sync::Arc;

// ============================================================================
// The public API: what code outside the engine may name. Every submodule but
// `host` (the seams the host answers) is private, so the compiler keeps
// callers on this list; behavior is `Operations` methods, these are the types
// they take and return, plus the few engine-free helpers.
// ============================================================================

// The handle and its builder.
pub(crate) use handle::OperationsBuilder;
/// Why a second `Operations::claim_ledger` refused.
pub(crate) use in_flight_temps::AlreadyClaimed;

// The two event sink seams. The app builds its Tauri-backed answers
// (`operations_host::sinks`) at the IPC edge and hands them to a start; the
// engine itself never constructs a sink. `write-conflict` carries an
// `ArmedConflict`.
pub use conflict_slot::ArmedConflict;
pub use event_sinks::{OperationEventSink, ScanPreviewEventSink};

// The queue: `operations-changed`'s payload and what the queue controls answer.
pub use manager::{
    OperationSnapshot, OperationsChanged, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome,
};

// The live transfer tree's one IPC answer: a window of rows for the dialog and
// the queue (`transfer_tree/CLAUDE.md`), whose rows `cmdr://state` reads too.
pub(crate) use transfer_tree::{TransferTreeRow, TransferTreeView, TransferTreeViewRequest, TreeRowKind, TreeRowState};
pub(crate) use tree_view::TransferTreeViewError;

// Instant mutations: the typed refusal, the reply-within-a-deadline contract
// (`mutation_reply.rs`), and rename's batch and validity vocabulary.
pub use mutation_error::MutationError;
pub(crate) use mutation_reply::{MUTATION_REPLY_DEADLINE, Replied, reply_or_hand_off, reply_within};
pub use mutation_reply::{MutationReply, MutationSettled};
pub(crate) use rename::{BulkRenameRow, RenameStartError, RenameValidityResult, RenamesStarted};

// Pre-flight: the transfer dialog's conflict check, a preview's cost facts
// (the S3 estimate prices them), the source binding a reviewed batch supplies
// (`source_binding.rs`), and an undo's report.
pub(crate) use conflict_preflight::CONFLICT_CHECK_BUDGET;
pub use conflict_preflight::{SourceItemInput, VolumeScanError};
pub use rollback::UndoReport;
pub(crate) use scan_cache::ScanCostFacts;
pub(crate) use source_binding::{ExpectedSources, LocalContent, RemoteContent, SourceFingerprint};

// Engine-free helpers the IPC edge asks before or beside an operation: the F8
// routing question, where a path's trash is, the look-alike name check,
// whether two paths are one local file, and anchoring a volume-relative
// destination (`routing.rs`).
pub use delete::cloud_trash::{TrashRoutingAnswer, routing_for_selection as trash_routing_for_selection};
pub(crate) use delete::trash::trash_dir_for_path;
pub(crate) use look_alike::held_in_another_spelling;
pub(crate) use rename::same_local_file;
pub(crate) use routing::resolve_dest_path;

// The wire vocabulary: payloads, configs, and typed outcomes.
pub use types::{
    ConflictId, ConflictInfo, ConflictResolution, ConflictResolutionOutcome, DryRunResult, LifecycleStatus,
    MoveLeftoversKeptEvent, OperationStatus, ReadOnlySide, RetryOutcome, ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent, ScanPreviewPurpose, ScanPreviewRefusal,
    ScanPreviewStartResult, ScanPreviewTotals, ScanProgressEvent, SourceItemOutcome, SourcesGoneAnswerOutcome,
    TransferActivity, VolumeCopyConfig, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
    WriteSourcesGoneEvent,
};

// What the app-side test cells reach beyond the list above.
#[cfg(test)]
pub(crate) mod testing;

// ============================================================================
// Reach for the engine's own modules: siblings and the suites below name these
// at the root. Private, so nothing outside the engine can.
// ============================================================================

use summary::path_summary;
use transfer::volume::copy_volumes_with_progress;

// What `OperationSnapshot`'s field docs link to. Those docs reach `bindings.ts`
// verbatim, so the name comes to them rather than the other way round.
#[cfg(doc)]
use manager::OperationDescriptor;

#[cfg(test)]
use archive_edit::{EditError, pull_apply_upload_swap, route_archive_copy_into};
#[cfg(test)]
use delete::delete_volume_files_for_test;
#[cfg(test)]
use event_sinks::CollectorEventSink;
#[cfg(test)]
use scan_cache::seed_incoherent_scan_result_for_test;
#[cfg(test)]
use state::{OperationIntent, WriteOperationState};
#[cfg(test)]
use transfer::transfer_probe::render_live_dump as render_live_transfer_dump;
#[cfg(test)]
use transfer::volume::{move_volumes_with_progress, move_within_same_volume_with_progress};
#[cfg(test)]
use validation::{
    ensure_destination_dir, is_same_file, is_same_filesystem, validate_destination_not_inside_source,
    validate_destination_writable, validate_path_length, validate_source_names_are_distinct, validate_sources,
};

// ============================================================================
// The engine handle
// ============================================================================

/// The write engine: a cheap-to-clone handle, every clone sharing one state.
///
/// The host builds one with [`Operations::builder`] and holds it; a test or a
/// tool builds its own. Two engines share nothing: an operation registered in
/// one is invisible to the other.
#[derive(Clone)]
pub struct Operations {
    inner: Arc<Inner>,
}

/// The engine's state: the seams it asks, and everything it keeps between two
/// calls. One per [`Operations`]; nothing here is process-wide.
///
/// Defined here at the module root, the parent of every module that works on a
/// part of it, so those modules hang off one hub instead of circling through a
/// sibling.
pub(crate) struct Inner {
    /// Everything the engine reaches the host through, shared with each
    /// operation's scope.
    host: Arc<host::EngineHost>,
    /// The journal every operation this engine runs is recorded in.
    operation_log: cmdr_operation_log::OperationLog,
    /// Admission, lanes, and the queue's records.
    manager: Arc<manager::OperationManager>,
    /// Every live operation's state, by id.
    registry: Arc<state::WriteOperationRegistry>,
    /// Every registered operation's status, and the busy-volume set.
    status: Arc<status_cache::StatusCache>,
    /// The stores each operation's scope shares.
    stores: Arc<EngineStores>,
}

/// What the engine keeps between two calls that its operations reach through
/// their scope: the scan previews, the in-flight ledger, the lanes' outage
/// records, and the transfer probes.
///
/// Here at the hub, beside [`Inner`], for the same reason `Inner` is: the
/// modules that own each store also work on operation states, so a scope
/// naming those modules would circle back through `state`. The scope names
/// this type instead.
pub(in crate::file_system::write_operations) struct EngineStores {
    previews: Arc<scan_cache::ScanPreviews>,
    /// What this engine has on a disk that isn't the user's file yet. In
    /// memory until the engine's one claim points it at a data dir.
    ledger: in_flight_temps::Ledger,
    /// Set by the claim, once ever (`Operations::claim_ledger`).
    ledger_claimed: std::sync::atomic::AtomicBool,
    /// One outage record per lane, shared by every transfer waiting on it.
    outages: Arc<transfer::volume_outage::Outages>,
    /// Every live transfer's in-flight table, for its stall watchdog and the
    /// activity each progress event carries.
    probes: Arc<transfer::transfer_probe::Probes>,
}

impl EngineStores {
    fn new() -> Self {
        Self {
            previews: Arc::new(scan_cache::ScanPreviews::new()),
            ledger: in_flight_temps::Ledger::new(),
            ledger_claimed: std::sync::atomic::AtomicBool::new(false),
            outages: Arc::new(transfer::volume_outage::Outages::new()),
            probes: Arc::new(transfer::transfer_probe::Probes::new()),
        }
    }

    /// Every live transfer's probe, shared, since a registration's guard holds
    /// the table until it deregisters.
    pub(in crate::file_system::write_operations) fn probes(&self) -> &Arc<transfer::transfer_probe::Probes> {
        &self.probes
    }

    /// The lanes' outage records, shared, since a waiter's membership holds
    /// them until it leaves.
    pub(in crate::file_system::write_operations) fn outages(&self) -> &Arc<transfer::volume_outage::Outages> {
        &self.outages
    }

    /// The in-flight ledger every operation on this engine records into.
    pub(in crate::file_system::write_operations) fn ledger(&self) -> &in_flight_temps::Ledger {
        &self.ledger
    }

    /// Takes the ledger's one claim: `true` the first time, `false` ever after.
    pub(in crate::file_system::write_operations) fn take_ledger_claim(&self) -> bool {
        !self.ledger_claimed.swap(true, std::sync::atomic::Ordering::SeqCst)
    }

    /// Every scan preview this engine knows about, in flight or settled.
    pub(in crate::file_system::write_operations) fn previews(&self) -> &scan_cache::ScanPreviews {
        &self.previews
    }

    /// The same, shared, for a part of the engine that keeps its own handle on
    /// it (the manager).
    pub(in crate::file_system::write_operations) fn previews_arc(&self) -> &Arc<scan_cache::ScanPreviews> {
        &self.previews
    }
}

/// The engine the suites and the bench share, built on first read: in a test
/// binary the app's answers with no frontend (`test_support::shared_engine`,
/// which `operations_host::operations()` hands out there too), in the bench a
/// detached one. ❌ Production code has no such accessor: the host holds its
/// engine and passes it in.
// shim: removed in M7b–M7e
#[cfg(any(test, feature = "transfer-bench"))]
pub(crate) fn installed() -> &'static Operations {
    static SHARED: std::sync::OnceLock<Operations> = std::sync::OnceLock::new();
    #[cfg(test)]
    {
        SHARED.get_or_init(test_support::shared_engine)
    }
    #[cfg(not(test))]
    {
        SHARED.get_or_init(Operations::detached)
    }
}

// Every backend's cells through this pipeline, and the scenarios they share.
#[cfg(test)]
mod approved_op_parity_tests;
#[cfg(test)]
pub(crate) mod backend_suites;
#[cfg(test)]
mod journal_capture_tests;
#[cfg(test)]
mod journal_capture_volume_tests;
// Every write path announces each own write before the syscall that makes it.
#[cfg(test)]
mod own_write_announce_tests;
#[cfg(test)]
mod scan_bridge_tests;
// A walk that stops on a missing or unreadable item fails its operation naming that item.
#[cfg(test)]
mod scan_failure_tests;
#[cfg(test)]
mod scan_pause_tests;
#[cfg(test)]
mod scan_preview_listing_progress_tests;
#[cfg(test)]
mod scan_preview_oracle_tests;
#[cfg(test)]
mod scan_preview_tests;
// A transfer's preview builds the operation's live tree, and only a transfer's.
#[cfg(test)]
mod scan_preview_tree_tests;
#[cfg(test)]
mod scan_watchdog_tests;
#[cfg(test)]
mod settle_event_tests;
// The one cooperative-stop boundary every serial loop here asks at.
#[cfg(test)]
mod free_space_tests;
#[cfg(test)]
mod stop_or_park_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_integration_test;
