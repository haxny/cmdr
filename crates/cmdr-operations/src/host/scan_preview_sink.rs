//! The per-preview seam: what one scan preview reports while it walks.

use super::super::types::{
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
};

/// Abstraction for emitting the four SCAN PREVIEW events.
///
/// Same reason `OperationEventSink` exists, applied to the pre-confirm dialog's
/// own counters: the preview workers and the watchdog that bounds them hold a
/// sink rather than a `tauri::AppHandle`, so a test can point a walk at a volume
/// that never answers and watch the preview settle anyway. Without the seam that
/// case is only reachable through a real window. The app's answer is
/// `TauriScanPreviewSink` (`operations_host::sinks`).
pub trait ScanPreviewEventSink: Send + Sync + 'static {
    /// The walk's running counts.
    fn emit_progress(&self, event: ScanPreviewProgressEvent);
    /// The walk finished.
    fn emit_complete(&self, event: ScanPreviewCompleteEvent);
    /// The walk failed or timed out.
    fn emit_error(&self, event: ScanPreviewErrorEvent);
    /// The walk was cancelled.
    fn emit_cancelled(&self, event: ScanPreviewCancelledEvent);
}
