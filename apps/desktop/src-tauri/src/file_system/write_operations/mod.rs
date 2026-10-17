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
pub(crate) use handle::OperationsBuilder;
// Two engines share nothing.
#[cfg(test)]
mod handle_tests;
pub(crate) mod host;
mod human_wait;
mod in_flight_temps;
mod journal;
mod journal_search;
// The local-filesystem copy, move, delete, and trash starters.
mod local_start;
#[cfg(test)]
pub(crate) use journal_search::SEARCH_LEAF_CAP;
#[cfg(test)]
pub(crate) use local_start::copy_files_start;
pub use local_start::{delete_files_start, move_files_start, trash_files_start};
mod ledger;
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
pub(crate) mod rollback;
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
pub(crate) use summary::path_summary;
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

// The live transfer tree's one IPC answer: a window of rows for the dialog and
// the queue (`transfer_tree/CLAUDE.md`), whose rows `cmdr://state` reads too.
#[cfg(test)]
pub(crate) use transfer_tree::TransferTreeAncestor;
pub(crate) use transfer_tree::{TransferTreeRow, TransferTreeView, TransferTreeViewRequest, TreeRowKind, TreeRowState};
pub(crate) use tree_view::{TransferTreeViewError, transfer_tree_view};

// Re-export `trash` at this level so `crate::file_system::write_operations::trash`
// keeps resolving (used by `commands/rename.rs`).
pub(crate) use delete::trash;

// The F8 routing question, asked by the IPC command before the confirmation dialog opens.
pub use delete::cloud_trash::{TrashRoutingAnswer, routing_for_selection as trash_routing_for_selection};

use std::sync::Arc;

// The two event sink seams. The app builds its Tauri-backed answers
// (`operations_host::sinks`) at the IPC edge and injects them into the managed
// pipeline; the pipeline itself never constructs a sink.
pub use event_sinks::{OperationEventSink, ScanPreviewEventSink};

// Re-export public types
/// The quit teardown's fence over that same ledger: everything recorded is with
/// the kernel before the process ends, so the next launch's sweep sees it.
pub use in_flight_temps::flush as flush_in_flight_temps;
/// Points the in-flight transfer-partial ledger at the app data dir and clears
/// what an earlier run left behind. Startup only, before any copy can start.
pub use in_flight_temps::init_and_sweep as init_and_sweep_in_flight_temps;
pub(crate) use scan_cache::{ScanCostFacts, cached_cost_facts};
pub use scan_preview::{cancel_scan_preview, get_scan_preview_totals, start_scan_preview};
pub use state::{
    busy_volume_ids, cancel_all_write_operations, cancel_write_operation, get_operation_status, pending_write_conflict,
    resolve_write_conflict,
};
// A queue window that opens late seeds its rows from this (a paused operation emits nothing).
pub use state::latest_operation_progress;
// The hard-abort tier. Exactly one legitimate caller: the quit deadline
// (`crate::quit`), which fires it only after the cooperative cancel has had its
// window. ❌ Never reach for it from anything a person clicked — it skips the
// backend's own partial cleanup. `transfer/DETAILS.md` § "Two tiers of cancel".
pub use state::abort_all_write_operations;
// Operation manager: the single scheduler + registry every write op flows
// through. `OperationsChanged` / `OperationSnapshot` are the thin
// `operations-changed` event payload (the queue window consumes them). The
// `LifecycleStatus` they carry is vocabulary and lives in `types`. The
// command helpers back the `list_operations` / `cancel_operation(s)` IPC.
// A queued operation whose sources left while it waited asks before carrying on.
pub use manager::{
    OperationSnapshot, OperationsChanged, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome,
    cancel_operation, cancel_operations, dismiss_all_failed_operations, dismiss_failed_operation, list_operations,
    move_queued_operation, operations_need_volume, pause_all, pause_operation, place_queued_operation, resume_all,
    resume_operation, retry_failed_operation,
};
pub use queued_sources::continue_without_gone_sources;
pub(crate) use queued_sources::pending_sources_gone;
// Managed instant mutations (rename / mkdir / mkfile) + rename validation. The
// thin IPC commands (`commands/rename.rs`, `commands/file_system/write_ops.rs`)
// call these; `RenameValidityResult` rides into `bindings.ts` via the
// `check_rename_validity` command signature.
pub(crate) use create::{create_directory_managed, create_file_managed};
// The transfer and compress dialogs' destination probe (`destination_exists`).
#[cfg(target_os = "macos")]
pub(crate) use create_numbered::create_numbered_file;
pub(crate) use look_alike::held_in_another_spelling;
pub(crate) use rename::{
    BulkRenameRow, RenameStartError, RenameValidityResult, RenamesStarted, check_rename_permission_for_volume,
    check_rename_validity_impl, rename_managed, same_local_file, start_renames,
};
// How a backend caller outside the engine words a source volume nothing has registered.
pub(crate) use transfer::volume::unregistered_source_error;
// The batch executor alone, for the suites that drive it without the routing
// `start_renames` puts in front of it.
#[cfg(test)]
pub(crate) use rename::start_bulk_rename;
// The source-identity binding a reviewed batch may supply. `source_binding.rs`.
// Volume + destination resolution and the three routed cross-volume entry points,
// reachable by a backend caller and not only the IPC edge. `routing.rs`.
pub(crate) use routing::{
    resolve_dest_path, resolve_source_volume, start_rename_by_move, start_volume_compress, start_volume_copy,
    start_volume_move,
};
pub(crate) use source_binding::{ExpectedSources, LocalContent, RemoteContent, SourceFingerprint};
// Test-only reach for the suggestion bridge's suite, which checks that a binding captured at
// preflight refuses a source rewritten afterwards. Production callers get the pre-flight
// through the starters, never directly.
#[cfg(test)]
pub(crate) use source_binding::retain_bound_sources;
// External busy-volume seam for the drag-out fulfillment service (see
// `lifecycle/state.rs` § "External busy-volume seam"). `pub(crate)` so only in-crate
// callers (`native_drag::fulfillment`) reach it. macOS-only: the sole consumer
// (`native_drag`) is `#[cfg(target_os = "macos")]`, so on other targets these
// would be dead code under `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) use state::{register_external_volume_op, release_external_volume_op};
#[allow(unused_imports, reason = "Public API re-exports for consumers of this module")]
pub use types::{
    CancelRollback, ConflictId, ConflictInfo, ConflictResolution, ConflictResolutionOutcome, DryRunResult, ItemKinds,
    LifecycleStatus, MoveLeftoversKeptEvent, OperationStatus, ProgressAxis, ReadOnlySide, RetryOutcome,
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
    ScanPreviewPurpose, ScanPreviewRefusal, ScanPreviewStartResult, ScanPreviewTotals, ScanProgressEvent,
    SourceItemOutcome, SourcesGoneAnswerOutcome, SourcesGoneNext, SpaceShortfall, TransferActivity, TransferWaitReason,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};

// Re-export for tests (these are pub(crate) in validation.rs and state.rs)
#[cfg(test)]
pub(crate) use state::{OperationIntent, WriteOperationState, is_cancelled, load_intent};
#[cfg(test)]
#[allow(unused_imports, reason = "Re-exports for test modules in file_system")]
pub(crate) use validation::{
    ensure_destination_dir, is_same_file, is_same_filesystem, validate_destination_not_inside_source,
    validate_destination_writable, validate_path_length, validate_source_names_are_distinct, validate_sources,
};
// Exposed for the integration suites that drive `copy_volumes_with_progress`
// directly against a real backend instead of through the full Tauri path (for
// example `smb_stress_test`'s concurrent-copy cross-contamination cell).
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the SMB integration suites in this module")]
pub(crate) use event_sinks::CollectorEventSink;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the volume-journal capture tests")]
pub(crate) use transfer::volume::move_volumes_with_progress;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the SMB integration suites in this module")]
pub(crate) use transfer::volume::move_within_same_volume_with_progress;
// The four real-SMB safety cells drive the same axes the in-memory grid does:
// a cache entry that counted files but recorded no per-source result, and a
// volume that stops answering partway through.
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the SMB integration suites in this module")]
pub(crate) use delete::delete_volume_files_for_test;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the SMB integration suites in this module")]
pub(crate) use scan_cache::seed_incoherent_scan_result_for_test;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the SMB integration suites in this module")]
pub(crate) use transfer::volume::{FaultyOp, FaultyVolume};

/// Test-only: retain a failure straight in the manager, so a suite outside this
/// module can exercise a CONSUMER of `list_operations` (the MCP `queue` guard)
/// against a failed row without spawning a real operation. Drop it again with
/// the ordinary [`dismiss_failed_operation`].
#[cfg(test)]
pub(crate) fn test_retain_failure(operation_id: &str, operation_type: WriteOperationType, error: &WriteOperationError) {
    installed().retain_failure(operation_id, operation_type, error);
}

// Re-export volume copy types and functions. `routing.rs` owns which of
// `copy_between_volumes` / `move_between_volumes` / `route_archive_{copy_into,
// move_out}` / `compress_start` a given transfer reaches, so those four are
// reached through `super::` inside this module and are NOT re-exported: one
// routing, one place to keep it right.
pub use mutation_error::MutationError;
#[cfg(test)]
pub(crate) use mutation_reply::MutationSettledOutcome;
pub(crate) use mutation_reply::{MUTATION_REPLY_DEADLINE, Replied, reply_or_hand_off, reply_within};
pub use mutation_reply::{MutationReply, MutationSettled};
pub use types::VolumeCopyConfig;
// The transfer dialog's pre-flight conflict check: `commands/file_system/
// volume_copy.rs`'s `scan_volume_for_conflicts` is a thin wrapper around
// `scan_volume_for_conflicts_within`, budgeted the same way as every other
// IPC-facing scan. `conflict_preflight.rs`.
pub(crate) use conflict_preflight::{CONFLICT_CHECK_BUDGET, scan_volume_for_conflicts_within};
pub use conflict_preflight::{SourceItemInput, VolumeScanError};
// Test-only: the archive-edit and remote-transfer suites drive these drivers
// directly, under a `CollectorEventSink` and without a routing decision to make.
#[cfg(test)]
pub(crate) use archive_edit::{compress_start, route_archive_copy_into, route_archive_move_out};
// The cross-volume copy body, reused as the extract phase of an out-of-zip MOVE
// (`route_archive_move_out`). Not spawn-managed itself — it runs inside the
// move-out op's deferred under the move op's id/state/sink.
pub(crate) use transfer::volume::copy_volumes_with_progress;
// The remote zip-edit orchestration (pull-local, apply, upload, swap). Exposed at
// crate scope for the live-SMB / MTP integration suites, which drive the real
// mechanism against a real remote volume. The managed driver reaches it inside
// `archive_edit`, so this re-export is test-only.
#[cfg(test)]
pub(crate) use archive_edit::{EditError, pull_apply_upload_swap};
// A live operation's in-flight table, so a suite that bounds its own wait can put
// the transfer probe's dump in its panic message. Used by the live-SMB
// full-concurrency suite, which sits outside this module.
#[cfg(test)]
pub(crate) use transfer::transfer_probe::render_live_dump as render_live_transfer_dump;

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
}

/// The engine the host installed, for the free functions callers outside the
/// engine still reach it through.
// shim: removed in M7a
static INSTALLED: std::sync::OnceLock<Operations> = std::sync::OnceLock::new();

/// Parks the host's engine for the free-function shims. A second call hands
/// its engine back untouched.
// shim: removed in M7a
pub(crate) fn install(operations: Operations) -> Result<(), Operations> {
    INSTALLED.set(operations)
}

/// The engine the host installed, for a free function that `asked`, or `None`
/// before `install`.
///
/// ❌ `None` never panics and never falls back to another engine (a detached
/// one would write with nobody journaling or hearing it). Each shim refuses
/// with a variant its caller already handles, or answers "nothing" where that's
/// the truth (no engine, no operations), and this logs the miss at error level.
/// The engine names no app reporter, so a log line is all it can raise; the
/// error report comes from the app's own accessor
/// (`operations_host::operations`), which the IPC start commands go through.
/// The app installs at the top of `setup()`, so neither is reachable there.
///
/// A test binary never installs, so its first read builds the engine every
/// test shares (`test_support::shared_engine`); the bench binary's builds a
/// detached one.
// shim: removed in M7a
pub(crate) fn installed_for(asked: &'static str) -> Option<&'static Operations> {
    let engine = installed_slot();
    if engine.is_none() {
        log::error!(target: "op_manager", "{asked} reached the write engine before it was installed; refused");
    }
    engine
}

// shim: removed in M7a
fn installed_slot() -> Option<&'static Operations> {
    #[cfg(test)]
    {
        Some(INSTALLED.get_or_init(test_support::shared_engine))
    }
    #[cfg(all(not(test), feature = "transfer-bench"))]
    {
        Some(INSTALLED.get_or_init(Operations::detached))
    }
    #[cfg(all(not(test), not(feature = "transfer-bench")))]
    {
        INSTALLED.get()
    }
}

/// The engine the suites and the bench share, which always exists there (see
/// [`installed_slot`]). ❌ Production code has no such accessor: it goes through
/// [`installed_for`] and answers `None`.
// shim: removed in M7b–M7e
#[cfg(any(test, feature = "transfer-bench"))]
pub(crate) fn installed() -> &'static Operations {
    installed_slot().expect("a test or bench binary builds its engine on first read")
}

// ============================================================================
// What a shim answers before install
// ============================================================================

/// What a start answers before `install`: the frontend's generic I/O failure,
/// which its error dialog renders without naming a file (the app's IPC gate
/// refuses with the same, `operations_host::refuse_not_installed`).
// shim: removed in M7a
pub(crate) fn not_installed() -> WriteOperationError {
    WriteOperationError::IoError {
        path: String::new(),
        message: "the write engine isn't ready yet".to_string(),
    }
}

/// What an instant op (rename, a new folder or file, a single trash) answers
/// before `install`: "Cmdr is still starting up", the one existing refusal that
/// says so. Its copy names archive edits, the only way an instant op met it
/// before; a new variant would change the wire and owe new copy for a state the
/// app can't reach.
// shim: removed in M7a
pub(crate) fn instant_not_installed() -> MutationError {
    MutationError::ArchiveEditNotReady
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
