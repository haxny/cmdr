//! "Send feedback" Tauri command. Thin IPC wrapper; the validation, payload assembly, and
//! network logic live in [`crate::feedback`].

use crate::error_reporter::preview_file::{self, ReportPreviewError, ReportPreviewFile};
use crate::feedback::{self, SendFeedbackResult};

/// Server URL for feedback ingestion. Debug builds hit the local Worker; release hits production.
#[cfg(debug_assertions)]
const FEEDBACK_URL: &str = "http://localhost:8787/feedback";
#[cfg(not(debug_assertions))]
const FEEDBACK_URL: &str = "https://api.getcmdr.com/feedback";

/// Sends a beta tester's feedback text (plus an optional reply-to email they chose to attach,
/// and the past five hours of logs when they ticked that box) to the api-server. Returns a
/// typed result the UI branches on.
///
/// The feedback POST is network-only, so no `blocking_with_timeout` is needed (that's for
/// syscalls that hang on dead mounts); its `reqwest` client carries its own 10 s timeout. The
/// logs reuse the error reporter's bundle builder, which reads only the app's own log dir.
#[tauri::command]
#[specta::specta]
pub async fn send_feedback(
    app: tauri::AppHandle,
    feedback_text: String,
    email: Option<String>,
    include_logs: bool,
    logs_report_id: Option<String>,
) -> SendFeedbackResult {
    let text = match feedback::prepared_feedback_text(&feedback_text) {
        Ok(t) => t,
        Err(e) => {
            log::warn!(target: "cmdr_lib::feedback", "Feedback text rejected locally: {e:?}");
            return SendFeedbackResult::Invalid;
        }
    };
    feedback::send_with_optional_logs(&app, text, email, include_logs, logs_report_id, FEEDBACK_URL).await
}

/// Writes the logs the feedback would carry as one text file for the File viewer. Nothing leaves
/// the Mac. Pass the `id` an earlier look returned to rewrite the same report; `send_feedback`
/// takes it as `logs_report_id`, so the logs ship under the id (and redaction) that was viewed.
#[tauri::command]
#[specta::specta]
pub async fn write_feedback_logs_preview_file(
    app: tauri::AppHandle,
    id: Option<String>,
) -> Result<ReportPreviewFile, ReportPreviewError> {
    preview_file::write_preview(&app, feedback::logs_request(id)).await
}
