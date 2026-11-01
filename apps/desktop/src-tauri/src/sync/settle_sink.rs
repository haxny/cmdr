//! The sink a sync batch hands the write engine: it WRAPS the app's own sink
//! (`TauriEventSink`), so the queue window, progress, and every question the
//! engine asks reach the person exactly as for any copy, and on the side it
//! records each source's outcome and wakes the adapter once the op settles.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operations::{
    ArmedConflict, ConflictInfo, DryRunResult, OperationEventSink, ScanProgressEvent, SourceItemOutcome,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteLockedSourcesEvent,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};
use tokio::sync::oneshot;

/// One source's last word from the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceEnd {
    /// How it ended.
    pub(crate) outcome: SourceItemOutcome,
    /// Whether it is gone from where it was.
    pub(crate) removed: bool,
}

/// What one settled operation said.
#[derive(Debug, Clone, Default)]
pub(crate) struct Settled {
    /// The last outcome per source path (a cross-volume move speaks twice).
    pub(crate) ends: HashMap<PathBuf, SourceEnd>,
    /// It ended on a cancel.
    pub(crate) cancelled: bool,
}

impl Settled {
    /// The engine's last word on `path`.
    pub(crate) fn end_of(&self, path: &Path) -> Option<SourceEnd> {
        self.ends.get(path).copied()
    }
}

/// Forwards every event to `inner` untouched, records outcomes, and resolves
/// its receiver on `emit_settled`.
pub(crate) struct SettleSink {
    inner: Arc<dyn OperationEventSink>,
    settled: Mutex<Settled>,
    done: Mutex<Option<oneshot::Sender<Settled>>>,
}

impl SettleSink {
    /// A sink over `inner`, and the receiver that answers once the op settles.
    pub(crate) fn wrap(inner: Arc<dyn OperationEventSink>) -> (Arc<Self>, oneshot::Receiver<Settled>) {
        let (tx, rx) = oneshot::channel();
        let sink = Arc::new(Self {
            inner,
            settled: Mutex::new(Settled::default()),
            done: Mutex::new(Some(tx)),
        });
        (sink, rx)
    }

    /// What it has heard so far (for an op that left the queue unsettled).
    pub(crate) fn heard(&self) -> Settled {
        self.settled.lock_ignore_poison().clone()
    }
}

impl OperationEventSink for SettleSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner.emit_complete(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.settled.lock_ignore_poison().cancelled = true;
        self.inner.emit_cancelled(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner.emit_error(event);
    }
    fn emit_conflict(&self, event: ArmedConflict) {
        self.inner.emit_conflict(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        let end = SourceEnd {
            outcome: event.outcome,
            removed: event.source_removed,
        };
        self.settled
            .lock_ignore_poison()
            .ends
            .insert(PathBuf::from(&event.source_path), end);
        self.inner.emit_source_item_done(event);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        self.inner.emit_sources_gone(event);
    }
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        self.inner.emit_locked_sources(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.inner.emit_settled(event);
        let heard = self.heard();
        if let Some(tx) = self.done.lock_ignore_poison().take() {
            // The adapter may have stopped waiting (it gave up on an op that left the queue).
            let _ = tx.send(heard);
        }
    }
    fn note_source_landed_clean(&self, source: &Path) {
        self.inner.note_source_landed_clean(source);
    }
}
