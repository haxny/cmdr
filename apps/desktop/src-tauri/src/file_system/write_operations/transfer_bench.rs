//! The engine's hot paths, opened to the `transfer_benchmarks` Criterion bench.
//!
//! A bench compiles against the app as an EXTERNAL crate, so it sees neither
//! `#[cfg(test)]` nor `pub(crate)`, and `file_system` is private. This module is
//! the one door, behind the `transfer-bench` feature (re-exported at the crate
//! root as `cmdr_lib::transfer_bench`), and every function in it is a thin
//! wrapper over a real entry point: the same calls a started operation makes,
//! with a no-op sink and a fresh, UNREGISTERED `WriteOperationState`, so a
//! bench never depends on the process-global operation registry.
//!
//! Why the feature and not a self dev-dependency: `Cargo.toml` § `[features]`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use uuid::Uuid;

use super::delete::delete_files_with_progress_inner;
use super::event_sinks::{OperationEventSink, ScanPreviewEventSink};
use super::journal;
use super::scan_cache::ScanPreviewState;
use super::scan_preview::run_scan_preview;
use super::scan_watchdog::{SCAN_INACTIVITY_LIMIT, ScanTarget, ScanWatchdog};
use super::state::WriteOperationState;
use super::transfer::copy::copy_files_with_progress_inner;
use super::transfer::volume::copy_volumes_with_progress;
use super::types::{
    ConflictInfo, DryRunResult, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanPreviewPurpose, ScanProgressEvent, VolumeCopyConfig, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_fs::volume::{DEFAULT_VOLUME_ID, Volume};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::capture::WriterJournal;
use cmdr_operation_log::store::{OperationLogStoreError, open_read_connection, read_operation_items};
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind};
use cmdr_operation_log::writer::OperationLogWriter;

pub use super::types::WriteOperationError;

/// A sink that drops every event: the bench measures the engine, not a
/// collector's `Vec` growing or a webview's IPC.
struct DiscardEvents;

impl OperationEventSink for DiscardEvents {
    fn emit_progress(&self, _event: WriteProgressEvent) {}
    fn emit_complete(&self, _event: WriteCompleteEvent) {}
    fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
    fn emit_error(&self, _event: WriteErrorEvent) {}
    fn emit_conflict(&self, _event: WriteConflictEvent) {}
    fn emit_conflict_resolved(&self, _event: WriteConflictResolvedEvent) {}
    fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _result: DryRunResult) {}
    fn emit_settled(&self, _event: WriteSettledEvent) {}
}

/// The scan preview's own four events, dropped the same way.
struct DiscardPreviewEvents;

impl ScanPreviewEventSink for DiscardPreviewEvents {
    fn emit_progress(&self, _event: ScanPreviewProgressEvent) {}
    fn emit_complete(&self, _event: ScanPreviewCompleteEvent) {}
    fn emit_error(&self, _event: ScanPreviewErrorEvent) {}
    fn emit_cancelled(&self, _event: ScanPreviewCancelledEvent) {}
}

/// An operation's state the way `start_write_operation` builds it, minus the
/// registration: the progress interval the frontend sends by default.
fn fresh_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(
        WriteOperationConfig::default().progress_interval_ms,
    )))
}

/// A local same-volume copy of `sources` into `destination`, through
/// `copy_files_with_progress_inner` with no journal.
pub fn copy_local(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
    let op_id = cmdr_operation_log::new_operation_id();
    copy_files_with_progress_inner(
        &DiscardEvents,
        &op_id,
        &fresh_state(),
        sources,
        destination,
        &WriteOperationConfig::default(),
    )
}

/// A local delete of `sources`, through `delete_files_with_progress_inner`.
pub fn delete_local(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    let op_id = cmdr_operation_log::new_operation_id();
    delete_files_with_progress_inner(
        &DiscardEvents,
        &op_id,
        &fresh_state(),
        sources,
        &WriteOperationConfig::default(),
    )
}

/// A local volume over `root`, the destination a cross-volume copy streams into,
/// on the host the engine itself runs on.
pub fn local_volume(root: impl Into<PathBuf>) -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        "Transfer bench",
        root,
        super::installed().host().volume_host().clone(),
    ))
}

/// A cross-volume copy through `copy_volumes_with_progress`: the chunked
/// `open_read_stream` → `write_from_stream` path. Needs a multi-threaded Tokio
/// runtime, like every volume backend.
pub async fn copy_volumes(
    source_volume: Arc<dyn Volume>,
    sources: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
) -> Result<(), WriteOperationError> {
    let op_id = cmdr_operation_log::new_operation_id();
    copy_volumes_with_progress(
        Arc::new(DiscardEvents),
        &op_id,
        &fresh_state(),
        source_volume,
        sources,
        dest_volume,
        dest_path,
        &VolumeCopyConfig::default(),
    )
    .await
    .map_err(|failure| failure.error)
}

/// The local scan-preview worker over `sources`, run inline on the calling
/// thread the way its spawned thread runs it, then the cached result consumed
/// the way a confirmed operation consumes it. Returns the file count the walk
/// cached, or `None` when it settled without a result.
///
/// Call it inside a Tokio runtime context: the preview's watchdog spawns its
/// heartbeat there.
pub fn scan_preview_local(sources: Vec<PathBuf>) -> Option<usize> {
    let engine = super::installed();
    let preview_id = Uuid::new_v4().to_string();
    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(WriteOperationConfig::default().progress_interval_ms),
    });
    let events: Arc<dyn ScanPreviewEventSink> = Arc::new(DiscardPreviewEvents);
    let watchdog = ScanWatchdog::start(
        engine.stores().previews_arc(),
        preview_id.clone(),
        ScanTarget::of(&sources, DEFAULT_VOLUME_ID),
        SCAN_INACTIVITY_LIMIT,
        Arc::clone(&state),
        Arc::clone(&events),
    );
    run_scan_preview(
        engine,
        events,
        preview_id.clone(),
        sources.clone(),
        state,
        ScanPreviewPurpose::Transfer,
        watchdog,
    );
    engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &sources)
        .map(|result| result.file_count)
}

/// A temp-DB operation log installed as THE process-global journal, so a local
/// copy journals the way a started one does. Dropping it uninstalls the
/// journal, so the unjournaled cases measured after it stay unjournaled.
pub struct BenchJournal {
    db_path: PathBuf,
}

impl BenchJournal {
    /// Spawns the writer over `db_path` (created if missing) and installs it.
    pub fn install(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        let writer = OperationLogWriter::spawn(db_path)?;
        super::installed()
            .operation_log()
            .set_journal(Some(Arc::new(WriterJournal::new(writer))));
        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    /// [`copy_local`], bracketed by the journal's open and finalize exactly as
    /// `start_write_operation` brackets it. Returns the operation's id, for
    /// [`Self::journaled_files`].
    pub fn copy_local(&self, sources: &[PathBuf], destination: &Path) -> Result<String, WriteOperationError> {
        let op_id = cmdr_operation_log::new_operation_id();
        let state = fresh_state();
        journal::open_local_op(
            state.scope(),
            &op_id,
            OpKind::Copy,
            Initiator::User,
            sources.len() as u64,
            Some(DEFAULT_VOLUME_ID),
        );
        let result = copy_files_with_progress_inner(
            &DiscardEvents,
            &op_id,
            &state,
            sources,
            destination,
            &WriteOperationConfig::default(),
        );
        let execution_status = match &result {
            Ok(()) => ExecutionStatus::Done,
            Err(WriteOperationError::Cancelled { .. }) => ExecutionStatus::Canceled,
            Err(_) => ExecutionStatus::Failed,
        };
        journal::finalize_op(state.scope(), OpKind::Copy, execution_status);
        result.map(|()| op_id)
    }

    /// How many file rows the journal holds for `op_id`: the proof a bench
    /// iteration journaled at all.
    pub fn journaled_files(&self, op_id: &str) -> Result<usize, OperationLogStoreError> {
        let conn = open_read_connection(&self.db_path)?;
        let items = read_operation_items(&conn, op_id, u32::MAX)?;
        Ok(items.iter().filter(|item| item.entry_type == EntryType::File).count())
    }
}

impl Drop for BenchJournal {
    fn drop(&mut self) {
        super::installed().operation_log().set_journal(None);
    }
}
