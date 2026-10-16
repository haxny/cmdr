//! Tauri commands for licensing.

use crate::licensing;

/// Get the current app status (personal, commercial, or expired).
#[tauri::command]
#[specta::specta]
pub fn get_license_status(app: tauri::AppHandle) -> licensing::AppStatus {
    licensing::get_app_status(&app)
}

/// Verify a license key or short code without writing anything to disk.
/// Returns the verify result (LicenseInfo + full key) for the frontend to inspect
/// before deciding whether to commit.
#[tauri::command]
#[specta::specta]
pub async fn verify_license(license_key: String) -> Result<licensing::VerifyResult, licensing::LicenseActivationError> {
    licensing::verify_license_async(&license_key).await
}

/// Persist a verified license key to the secret store and update caches.
/// Only call after verification confirms the key is valid.
///
/// On a blocking task with no deadline: the Keychain write can raise a consent dialog, which
/// must neither stall the main thread nor be cut off while the person reads it.
#[tauri::command]
#[specta::specta]
pub async fn commit_license(
    app: tauri::AppHandle,
    license_key: String,
    short_code: Option<String>,
) -> Result<licensing::LicenseInfo, licensing::LicenseActivationError> {
    tokio::task::spawn_blocking(move || licensing::commit_license(&app, &license_key, short_code.as_deref()))
        .await
        .map_err(|e| licensing::LicenseActivationError::StorageError {
            detail: format!("The license commit task stopped: {e}"),
        })?
}

/// Get information about the current license (if any).
#[tauri::command]
#[specta::specta]
pub fn get_license_info(app: tauri::AppHandle) -> Option<licensing::LicenseInfo> {
    licensing::get_license_info(&app)
}

/// Mark the expiration modal as shown (so it won't show again).
#[tauri::command]
#[specta::specta]
pub fn mark_expiration_modal_shown(app: tauri::AppHandle) {
    licensing::mark_expiration_modal_shown(&app);
}

/// Mark the commercial reminder as dismissed (resets the 30-day timer).
#[tauri::command]
#[specta::specta]
pub fn mark_commercial_reminder_dismissed(app: tauri::AppHandle) {
    licensing::mark_commercial_reminder_dismissed(&app);
}

/// Reset license data (debug builds only). On a blocking task, since it deletes the Keychain entry.
#[tauri::command]
#[specta::specta]
pub async fn reset_license(app: tauri::AppHandle) {
    if let Err(e) = tokio::task::spawn_blocking(move || licensing::reset_license(&app)).await {
        log::warn!("The license reset task stopped: {e}");
    }
}

/// Check if license needs re-validation with the server.
#[tauri::command]
#[specta::specta]
pub fn needs_license_validation(app: tauri::AppHandle) -> bool {
    licensing::needs_validation(&app)
}

/// Check if a server validation has ever completed for the current license.
#[tauri::command]
#[specta::specta]
pub fn has_license_been_validated(app: tauri::AppHandle) -> bool {
    licensing::has_been_validated(&app)
}

/// Validate license with server (async - call when needs_license_validation returns true).
/// If `transaction_id` is provided, uses it directly (for pre-commit validation).
/// If `None`, reads from the stored license (for periodic re-validation).
/// Returns Err on network/upstream errors so the frontend can distinguish from server rejection.
#[tauri::command]
#[specta::specta]
pub async fn validate_license_with_server(
    app: tauri::AppHandle,
    transaction_id: Option<String>,
) -> Result<licensing::AppStatus, String> {
    licensing::validate_license_async(&app, transaction_id.as_deref()).await
}
