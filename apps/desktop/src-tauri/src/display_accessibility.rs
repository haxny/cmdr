//! macOS Accessibility > Display options reader.
//!
//! Reads the three display options the UI mirrors (Reduce transparency,
//! Increase contrast, Differentiate without color) via `NSWorkspace`, and
//! observes `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification` to
//! emit live updates.
//!
//! ## Notes
//!
//! - WKWebView doesn't reflect `prefers-reduced-transparency` (verified: AppKit
//!   reports `true` while the webview's `matchMedia` reports `false`), and CSS
//!   has no media query for Differentiate without color at all, so the
//!   frontend reads these here and toggles classes on `<html>`.
//! - The accessibility-options notification posts on the `NSWorkspace`
//!   notification center (NOT the default center), so we subscribe there.
//! - It fires for the whole accessibility-display options group (also reduce
//!   motion, invert colors, etc.), so we always re-read every value rather
//!   than trusting the notification payload.

use std::ptr::NonNull;

use log::{debug, info, warn};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::NSNotification;
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

use crate::system_events::{DisplayAccessibility, DisplayAccessibilityChanged};

/// Reads the current display options.
///
/// `NSWorkspace` accessibility-display queries are AppKit-main-thread-only. The
/// `mtm: MainThreadMarker` is the compile-time proof we're on the main thread;
/// the objc2 0.3 methods don't take it as an argument, so its presence alone
/// is what makes an off-main call fail to compile.
fn read_display_accessibility(_mtm: MainThreadMarker) -> DisplayAccessibility {
    let workspace = NSWorkspace::sharedWorkspace();
    DisplayAccessibility {
        reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
        differentiate_without_color: workspace.accessibilityDisplayShouldDifferentiateWithoutColor(),
    }
}

/// Tauri command: returns the macOS Accessibility > Display options the UI mirrors.
///
/// `NSWorkspace` accessibility queries are main-thread-only, so we hop to the
/// AppKit main thread via `run_on_main_thread` and read there, mirroring
/// `get_accent_color`. Returns all-off (the safe default) if the main-thread hop
/// or channel fails.
#[tauri::command]
#[specta::specta]
pub fn get_display_accessibility(app: AppHandle) -> DisplayAccessibility {
    let (tx, rx) = std::sync::mpsc::channel();
    if app
        .run_on_main_thread(move || {
            let mtm = MainThreadMarker::new().expect("run_on_main_thread runs on the main thread");
            let _ = tx.send(read_display_accessibility(mtm));
        })
        .is_err()
    {
        warn!("Couldn't hop to the main thread to read display accessibility options, defaulting to all off");
        return DisplayAccessibility::default();
    }

    rx.recv().unwrap_or_else(|e| {
        warn!("Couldn't receive display accessibility options from the main thread ({e}), defaulting to all off");
        DisplayAccessibility::default()
    })
}

/// Starts observing `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`.
/// Emits `display-accessibility-changed` with the fresh options whenever the user
/// changes their Accessibility > Display options in System Settings.
pub fn observe_display_accessibility_changes<R: Runtime>(app_handle: AppHandle<R>) {
    // This runs at startup on the main thread (called from the Tauri setup hook).
    let mtm = MainThreadMarker::new().expect("observe_display_accessibility_changes runs on the main thread");
    let initial = read_display_accessibility(mtm);
    debug!("Display accessibility: {initial:?}");

    // Accessibility-display options post on the NSWorkspace notification center,
    // not the default center.
    let center = NSWorkspace::sharedWorkspace().notificationCenter();

    // Use block-based observer so we don't need a selector or ObjC class.
    // The block captures the app handle to emit Tauri events.
    let block = block2::RcBlock::new(move |_notification: NonNull<NSNotification>| {
        // NSNotificationCenter delivers this on the thread that posted the
        // notification; accessibility-options changes post on the main thread.
        let mtm = MainThreadMarker::new()
            .expect("NSWorkspaceAccessibilityDisplayOptionsDidChange is delivered on the main thread");
        let options = read_display_accessibility(mtm);
        info!("Display accessibility changed: {options:?}");
        if let Err(e) = (DisplayAccessibilityChanged { options }).emit(&app_handle) {
            warn!("Failed to emit display-accessibility-changed event: {e}");
        }
    });

    // SAFETY: NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification is a valid
    // notification name constant, and `center` is the live `NSWorkspace` notification
    // center. `block` is a live `RcBlock` with the expected
    // `(NonNull<NSNotification>) -> ()` signature. The observer is retained by the
    // center for the lifetime of the app; we intentionally never remove it because we
    // want updates for the entire session.
    unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
            None,
            &block,
        );
    }
}
