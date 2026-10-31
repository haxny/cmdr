//! The per-operation seam: what one operation reports while it runs.

use super::super::conflict_slot::ArmedConflict;
use super::super::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent,
    WriteErrorEvent, WriteLockedSourcesEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
    WriteSourcesGoneEvent,
};

/// Abstraction for emitting write operation events.
///
/// Decouples the copy/move/delete pipeline from `tauri::AppHandle`. The app
/// provides `TauriEventSink` (`operations_host::sinks`, calls `app.emit`). Tests
/// use `CollectorEventSink` (stores events in a `Vec` for assertions).
pub trait OperationEventSink: Send + Sync {
    /// A progress frame.
    fn emit_progress(&self, event: WriteProgressEvent);
    /// The operation finished.
    fn emit_complete(&self, event: WriteCompleteEvent);
    /// The operation stopped on a cancel or a rollback.
    fn emit_cancelled(&self, event: WriteCancelledEvent);
    /// The operation failed.
    fn emit_error(&self, event: WriteErrorEvent);
    /// A Stop-mode clash is on screen. Only an [`ArmedConflict`] goes out: the
    /// slot that answers it is armed before anyone can see it.
    fn emit_conflict(&self, event: ArmedConflict);
    /// The clash named by this event is over and the operation has carried on.
    /// Every surface showing it takes it down, including the ones that answered
    /// nothing (an agent over MCP answered, or another window won the race).
    /// Emit it from wherever an answer lands, ❌ never from the answering call:
    /// only the operation knows an answer actually reached it.
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent);
    /// One top-level source is done.
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent);
    /// A queued operation found some of its sources gone when its turn came
    /// (`queued_sources.rs`). Default no-op so the many single-purpose test
    /// sinks needn't spell it out; ❗ a sink that WRAPS another must forward it
    /// ([`SinkDecorator`] does), or the person is never asked.
    fn emit_sources_gone(&self, _event: WriteSourcesGoneEvent) {}
    /// A move found Locked items in what it moves and asks before any byte moves
    /// (`locked_sources.rs`). Default no-op like [`emit_sources_gone`](Self::emit_sources_gone),
    /// and ❗ for the same reason a sink that WRAPS another must forward it.
    fn emit_locked_sources(&self, _event: WriteLockedSourcesEvent) {}
    /// Per-iteration progress during dry-run scanning (separate from `write-progress`).
    fn emit_scan_progress(&self, event: ScanProgressEvent);
    /// One `ConflictInfo` per conflicting file during dry-run scanning.
    fn emit_scan_conflict(&self, conflict: ConflictInfo);
    /// Final dry-run result with conflict sample.
    fn emit_dry_run_complete(&self, result: DryRunResult);
    /// Emitted exactly once per op, after the spawned task fully returns
    /// (success, error, cancel, or panic). `WriteSettledGuard` calls this from
    /// its `Drop`, so every op settles through the injected sink. See
    /// `WriteSettledEvent` for the ordering contract.
    fn emit_settled(&self, event: WriteSettledEvent);

    /// Notes that a top-level source extracted in FULL (every file durably
    /// written, zero deep skips). Only the out-of-zip move op cares: it collects
    /// these to delete exactly the fully-extracted sources from the archive, so a
    /// partial move converges on retry. Default no-op for every other sink.
    fn note_source_landed_clean(&self, _source: &std::path::Path) {}
}

/// A sink that wraps another and changes only the events it overrides. Every
/// method forwards to [`inner`](Self::inner) by default, the ones
/// [`OperationEventSink`] defaults to a no-op included, so a wrapper can't drop
/// a parked question by forgetting to forward it. A blanket impl makes every
/// decorator an [`OperationEventSink`].
pub(crate) trait SinkDecorator: Send + Sync {
    /// The sink every event not overridden here goes to.
    fn inner(&self) -> &dyn OperationEventSink;

    /// [`OperationEventSink::emit_progress`], forwarded unless overridden.
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner().emit_progress(event);
    }
    /// [`OperationEventSink::emit_complete`], forwarded unless overridden.
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner().emit_complete(event);
    }
    /// [`OperationEventSink::emit_cancelled`], forwarded unless overridden.
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.inner().emit_cancelled(event);
    }
    /// [`OperationEventSink::emit_error`], forwarded unless overridden.
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner().emit_error(event);
    }
    /// [`OperationEventSink::emit_conflict`], forwarded unless overridden.
    fn emit_conflict(&self, event: ArmedConflict) {
        self.inner().emit_conflict(event);
    }
    /// [`OperationEventSink::emit_conflict_resolved`], forwarded unless overridden.
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner().emit_conflict_resolved(event);
    }
    /// [`OperationEventSink::emit_source_item_done`], forwarded unless overridden.
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner().emit_source_item_done(event);
    }
    /// [`OperationEventSink::emit_sources_gone`], forwarded unless overridden.
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        self.inner().emit_sources_gone(event);
    }
    /// [`OperationEventSink::emit_locked_sources`], forwarded unless overridden.
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        self.inner().emit_locked_sources(event);
    }
    /// [`OperationEventSink::emit_scan_progress`], forwarded unless overridden.
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner().emit_scan_progress(event);
    }
    /// [`OperationEventSink::emit_scan_conflict`], forwarded unless overridden.
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner().emit_scan_conflict(conflict);
    }
    /// [`OperationEventSink::emit_dry_run_complete`], forwarded unless overridden.
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner().emit_dry_run_complete(result);
    }
    /// [`OperationEventSink::emit_settled`], forwarded unless overridden.
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.inner().emit_settled(event);
    }
    /// [`OperationEventSink::note_source_landed_clean`], forwarded unless overridden.
    fn note_source_landed_clean(&self, source: &std::path::Path) {
        self.inner().note_source_landed_clean(source);
    }
}

impl<T: SinkDecorator> OperationEventSink for T {
    fn emit_progress(&self, event: WriteProgressEvent) {
        SinkDecorator::emit_progress(self, event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        SinkDecorator::emit_complete(self, event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        SinkDecorator::emit_cancelled(self, event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        SinkDecorator::emit_error(self, event);
    }
    fn emit_conflict(&self, event: ArmedConflict) {
        SinkDecorator::emit_conflict(self, event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        SinkDecorator::emit_conflict_resolved(self, event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        SinkDecorator::emit_source_item_done(self, event);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        SinkDecorator::emit_sources_gone(self, event);
    }
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        SinkDecorator::emit_locked_sources(self, event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        SinkDecorator::emit_scan_progress(self, event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        SinkDecorator::emit_scan_conflict(self, conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        SinkDecorator::emit_dry_run_complete(self, result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        SinkDecorator::emit_settled(self, event);
    }
    fn note_source_landed_clean(&self, source: &std::path::Path) {
        SinkDecorator::note_source_landed_clean(self, source);
    }
}
