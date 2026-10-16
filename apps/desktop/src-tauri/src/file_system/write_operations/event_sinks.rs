//! Event sinks: the test implementations of the two sink seams
//! (`CollectorEventSink`, `CollectorScanPreviewSink`, which store events in a
//! `Vec` for assertions), plus the event-builder `impl` blocks for
//! `WriteProgressEvent` and `WriteErrorEvent`.
//!
//! The traits themselves are seams and live in `host/`; they're re-exported here
//! so their many callers keep their path. The app's Tauri-backed answers live in
//! `operations_host::sinks`.

#[cfg(test)]
use cmdr_fs::ignore_poison::IgnorePoison;

pub use super::host::{OperationEventSink, ScanPreviewEventSink};
#[cfg(test)]
use super::types::{
    ConflictInfo, DryRunResult, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};
use super::types::{WriteErrorEvent, WriteOperationError, WriteOperationType, WriteProgressEvent};
use cmdr_index::ExpectedTotals;

impl WriteProgressEvent {
    /// Construct an event with the 8 core counter fields. Rate/ETA fields are
    /// filled in by `WriteOperationState::enrich_progress` right before the
    /// event is emitted, as is `activity`. The scanning-only metadata (`current_dir`,
    /// `expected_files_total`, `expected_bytes_total`) defaults to `None` and
    /// is populated by the scan emit sites via `with_scan_meta`. Always go
    /// through this constructor at emit sites so the extra fields stay out of
    /// call sites as visual noise.
    #[allow(
        clippy::too_many_arguments,
        reason = "These are the natural fields of a progress event. Bundling into a struct adds ceremony without cleaning anything up."
    )]
    pub fn new(
        operation_id: String,
        operation_type: WriteOperationType,
        phase: super::types::WriteOperationPhase,
        current_file: Option<String>,
        files_done: usize,
        files_total: usize,
        bytes_done: u64,
        bytes_total: u64,
    ) -> Self {
        Self {
            operation_id,
            operation_type,
            phase,
            current_file,
            current_dir: None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            dirs_done: 0,
            bytes_per_second: None,
            files_per_second: None,
            eta_seconds: None,
            bottleneck: None,
            expected_files_total: None,
            expected_bytes_total: None,
            activity: None,
            step: None,
        }
    }

    /// Marks which step of a multi-step operation this event's phase belongs to.
    #[must_use]
    pub fn with_step(mut self, step: Option<super::types::ProgressStep>) -> Self {
        self.step = step;
        self
    }

    /// Attach scanning-phase metadata (current directory, running dirs count,
    /// and index-derived expected totals) to an event. Only emit sites in the
    /// scanning phase call this; everywhere else leaves the fields at their
    /// defaults (`None` / `0`).
    #[must_use]
    pub fn with_scan_meta(
        mut self,
        current_dir: Option<String>,
        dirs_done: usize,
        expected: Option<ExpectedTotals>,
    ) -> Self {
        self.current_dir = current_dir;
        self.dirs_done = dirs_done;
        if let Some(e) = expected {
            self.expected_files_total = Some(e.files);
            self.expected_bytes_total = Some(e.bytes);
        }
        self
    }
}

impl WriteErrorEvent {
    /// Construct a `WriteErrorEvent` from the typed `error`. The FE renders all
    /// user-facing copy and the category/retry classification from the typed
    /// variant; no rendered prose crosses IPC.
    pub fn new(operation_id: String, operation_type: WriteOperationType, error: WriteOperationError) -> Self {
        Self {
            operation_id,
            operation_type,
            error,
            progress_at_stop: None,
        }
    }

    /// Attaches how far the operation had got. Built by
    /// [`super::transfer_sides::transfer_stop_event`], which reads the status
    /// cache while the operation is still registered.
    pub(super) fn with_progress_at_stop(mut self, progress: Option<super::types::ProgressAtStop>) -> Self {
        self.progress_at_stop = progress;
        self
    }
}

/// Test scan-preview sink: stores events for inspection.
#[cfg(test)]
pub(crate) struct CollectorScanPreviewSink {
    pub progress: std::sync::Mutex<Vec<ScanPreviewProgressEvent>>,
    pub complete: std::sync::Mutex<Vec<ScanPreviewCompleteEvent>>,
    pub errors: std::sync::Mutex<Vec<ScanPreviewErrorEvent>>,
    pub cancelled: std::sync::Mutex<Vec<ScanPreviewCancelledEvent>>,
}

#[cfg(test)]
impl CollectorScanPreviewSink {
    pub(crate) fn new() -> Self {
        Self {
            progress: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            errors: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl ScanPreviewEventSink for CollectorScanPreviewSink {
    fn emit_progress(&self, event: ScanPreviewProgressEvent) {
        self.progress.lock_ignore_poison().push(event);
    }
    fn emit_complete(&self, event: ScanPreviewCompleteEvent) {
        self.complete.lock_ignore_poison().push(event);
    }
    fn emit_error(&self, event: ScanPreviewErrorEvent) {
        self.errors.lock_ignore_poison().push(event);
    }
    fn emit_cancelled(&self, event: ScanPreviewCancelledEvent) {
        self.cancelled.lock_ignore_poison().push(event);
    }
}

/// Test event sink: stores events for inspection.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "Fields are populated by emit_* methods; read in test assertions as needed"
)]
pub(crate) struct CollectorEventSink {
    pub progress: std::sync::Mutex<Vec<WriteProgressEvent>>,
    pub complete: std::sync::Mutex<Vec<WriteCompleteEvent>>,
    pub cancelled: std::sync::Mutex<Vec<WriteCancelledEvent>>,
    pub errors: std::sync::Mutex<Vec<WriteErrorEvent>>,
    pub conflicts: std::sync::Mutex<Vec<WriteConflictEvent>>,
    pub conflicts_resolved: std::sync::Mutex<Vec<WriteConflictResolvedEvent>>,
    pub scan_progress: std::sync::Mutex<Vec<ScanProgressEvent>>,
    pub scan_conflicts: std::sync::Mutex<Vec<ConflictInfo>>,
    pub dry_run: std::sync::Mutex<Vec<DryRunResult>>,
    pub settled: std::sync::Mutex<Vec<WriteSettledEvent>>,
    pub source_items_done: std::sync::Mutex<Vec<WriteSourceItemDoneEvent>>,
    pub sources_gone: std::sync::Mutex<Vec<WriteSourcesGoneEvent>>,
}

#[cfg(test)]
impl CollectorEventSink {
    pub fn new() -> Self {
        Self {
            progress: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
            errors: std::sync::Mutex::new(Vec::new()),
            conflicts: std::sync::Mutex::new(Vec::new()),
            conflicts_resolved: std::sync::Mutex::new(Vec::new()),
            scan_progress: std::sync::Mutex::new(Vec::new()),
            scan_conflicts: std::sync::Mutex::new(Vec::new()),
            dry_run: std::sync::Mutex::new(Vec::new()),
            settled: std::sync::Mutex::new(Vec::new()),
            source_items_done: std::sync::Mutex::new(Vec::new()),
            sources_gone: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl OperationEventSink for CollectorEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.progress.lock_ignore_poison().push(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.complete.lock_ignore_poison().push(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.cancelled.lock_ignore_poison().push(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.errors.lock_ignore_poison().push(event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.conflicts.lock_ignore_poison().push(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.conflicts_resolved.lock_ignore_poison().push(event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.source_items_done.lock_ignore_poison().push(event);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        self.sources_gone.lock_ignore_poison().push(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.scan_progress.lock_ignore_poison().push(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.scan_conflicts.lock_ignore_poison().push(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.dry_run.lock_ignore_poison().push(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.settled.lock_ignore_poison().push(event);
    }
}
