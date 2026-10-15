//! Accessibility > Display options stub for non-macOS platforms.
//!
//! There's no cross-platform equivalent we read today, so every option reports
//! off (the safe default) and the observer is a no-op. The frontend still
//! honors the `prefers-contrast: more` media query where the webview reflects it.

use tauri::{AppHandle, Runtime};

use crate::system_events::DisplayAccessibility;

/// Returns every option off on non-macOS platforms.
#[tauri::command]
#[specta::specta]
pub fn get_display_accessibility() -> DisplayAccessibility {
    DisplayAccessibility::default()
}

/// No-op observer on non-macOS platforms; there's nothing to watch.
pub fn observe_display_accessibility_changes<R: Runtime>(_app_handle: AppHandle<R>) {}
