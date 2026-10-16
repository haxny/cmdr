//! The app's answers to the engine's two sink seams: each event becomes a Tauri
//! event, and a terminal one carries its app side effects first
//! (`terminal_effects.rs`).
//!
//! Built at the IPC edge, one per operation or preview, and handed to the
//! engine; the engine never builds one itself.

use tauri::AppHandle;
use tauri_specta::Event;

use super::terminal_effects::{self, AppTerminalEffects};
use crate::file_system::write_operations::{
    ConflictInfo, DryRunResult, OperationEventSink, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent,
    ScanPreviewErrorEvent, ScanPreviewEventSink, ScanPreviewProgressEvent, ScanProgressEvent, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent,
    WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent, operation_manager,
};

/// Tauri-backed event sink: calls `app.emit()` for each event.
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// The app side effects each terminal emit carries (`terminal_effects.rs`).
    fn effects(&self) -> AppTerminalEffects {
        AppTerminalEffects {
            manager: operation_manager(),
        }
    }
}

impl OperationEventSink for TauriEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        terminal_effects::complete(&self.effects(), event, |event| {
            let _ = event.emit(&self.app);
        });
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        terminal_effects::cancelled(&self.effects(), event, |event| {
            let _ = event.emit(&self.app);
        });
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        terminal_effects::error(&self.effects(), event, |event| {
            let _ = event.emit(&self.app);
        });
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        let _ = conflict.emit(&self.app);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        let _ = result.emit(&self.app);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        terminal_effects::settled(&self.effects(), event, |event| {
            let _ = event.emit(&self.app);
        });
    }
}

/// Tauri-backed scan-preview sink: calls `app.emit()` for each event.
pub struct TauriScanPreviewSink {
    app: AppHandle,
}

impl TauriScanPreviewSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ScanPreviewEventSink for TauriScanPreviewSink {
    fn emit_progress(&self, event: ScanPreviewProgressEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_complete(&self, event: ScanPreviewCompleteEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_error(&self, event: ScanPreviewErrorEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_cancelled(&self, event: ScanPreviewCancelledEvent) {
        let _ = event.emit(&self.app);
    }
}
