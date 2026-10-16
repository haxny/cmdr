//! "Delete all Cmdr data" (Settings › Updates & privacy). Pass-throughs to `crate::data_wipe`.

use std::time::Duration;

use tauri::AppHandle;

use crate::data_wipe::{DataWipeError, DataWipeSummary, WipeReport};

/// Adding up the sizes walks every folder, so it gets the recursive-scan tier.
const SUMMARY_DEADLINE: Duration = Duration::from_secs(30);

/// How much would go, and whether a license is activated here, for the confirmation dialog.
#[tauri::command]
#[specta::specta]
pub async fn data_wipe_summary(app: AppHandle) -> Result<DataWipeSummary, DataWipeError> {
    cmdr_fs::deadline::blocking_typed_result_with_timeout(
        SUMMARY_DEADLINE,
        || DataWipeError::TimedOut,
        |_| DataWipeError::Interrupted,
        move || Ok(crate::data_wipe::summary(&app)),
    )
    .await
}

/// Delete everything Cmdr keeps on this Mac, and say what couldn't go.
///
/// No deadline, on purpose: answering "timed out" while the delete carries on would tell the
/// person it stopped when it didn't. The dialog shows it running and can't be closed meanwhile.
#[tauri::command]
#[specta::specta]
pub async fn delete_all_app_data(app: AppHandle) -> Result<WipeReport, DataWipeError> {
    tauri::async_runtime::spawn_blocking(move || crate::data_wipe::delete_all(&app))
        .await
        .map_err(|_| DataWipeError::Interrupted)?
}

/// Quit after a wipe, through the quit gate like ⌘Q: `true` when the app is on its way out,
/// `false` when the gate held it because a file operation started since (it then asks).
#[tauri::command]
#[specta::specta]
pub fn quit_after_data_wipe(app: AppHandle) -> bool {
    match crate::quit::request_quit(&app) {
        crate::quit::QuitOutcome::Proceed => {
            app.exit(0);
            true
        }
        crate::quit::QuitOutcome::Held { .. } => false,
    }
}
