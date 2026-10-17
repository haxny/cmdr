//! The per-operation seam: what one operation reports while it runs.

use super::super::conflict_slot::ArmedConflict;
use super::super::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent,
    WriteErrorEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};

/// Abstraction for emitting write operation events.
///
/// Decouples the copy/move/delete pipeline from `tauri::AppHandle`. The app
/// provides `TauriEventSink` (`operations_host::sinks`, calls `app.emit`). Tests
/// use `CollectorEventSink` (stores events in a `Vec` for assertions).
pub trait OperationEventSink: Send + Sync {
    fn emit_progress(&self, event: WriteProgressEvent);
    fn emit_complete(&self, event: WriteCompleteEvent);
    fn emit_cancelled(&self, event: WriteCancelledEvent);
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
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent);
    /// A queued operation found some of its sources gone when its turn came
    /// (`queued_sources.rs`). Default no-op so the many single-purpose test
    /// sinks needn't spell it out; ❗ a sink that WRAPS another must forward it,
    /// or the person is never asked and the operation waits forever.
    fn emit_sources_gone(&self, _event: WriteSourcesGoneEvent) {}
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
