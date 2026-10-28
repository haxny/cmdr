//! What the app does as its windows and its process come and go.
//!
//! Two handlers, both wired into the Tauri builder from `lib.rs`: `on_window_event`
//! for per-window signals (focus, close, destroy) and `on_run_event` for
//! process-level ones (ready, exit requested, exit). `lib.rs` names them; the
//! decisions live here.
//!
//! The quit path deliberately runs through `quit::request_quit` in BOTH handlers:
//! closing the main window and quitting the process are separate signals that
//! reach the same gate, and either can be held while work is in flight. See
//! `quit/CLAUDE.md`.

use tauri::{AppHandle, Manager, Window, Wry};

use crate::downloads;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::network;
use crate::{ai, crash_reporter, data_wipe, file_viewer, mcp, quit, search, security_log, window_state};
#[cfg(target_os = "macos")]
use crate::{dock, drag_image_detection, mtp, reveal};

/// Stop the services that outlive a window: the local LLM, the MCP server, mDNS, and
/// Linux's mount watchers.
///
/// Reached from three places (main window closed, main window destroyed, process
/// exiting), because none of them implies the others fire: a `CloseRequested` the
/// gate waves through never becomes a `Destroyed` on some platforms, and an
/// `AppHandle::exit` skips the window events entirely. Every one of these calls is
/// idempotent, so overlapping paths cost nothing.
fn stop_background_services() {
    ai::manager::shutdown();
    mcp::stop_mcp_server();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    network::discovery_gate::shut_down();
    #[cfg(target_os = "linux")]
    crate::volumes_linux::watcher::stop_volume_watcher();
}

/// Per-window signals: focus re-checks the FDA gate, closing the main window quits
/// the app through the quit gate, and a destroyed viewer window frees its session.
pub fn on_window_event(window: &Window, event: &tauri::WindowEvent) {
    // Main-window focus re-checks the FDA gate so the Downloads
    // watcher starts/stops on transitions. Covers the "user
    // toggled FDA in System Settings, came back to Cmdr" path
    // without polling. Idempotent when nothing changed.
    if let tauri::WindowEvent::Focused(true) = event
        && window.label() == "main"
    {
        if let Err(err) = downloads::refresh_runtime(window.app_handle()) {
            log::warn!(
                target: "downloads::watcher",
                "Focus-driven gate re-check failed: {err}",
            );
        }
        // Re-evaluate the global-shortcut registration too: if FDA
        // flipped between blur and focus, register/unregister to
        // match. Idempotent when nothing changed.
        downloads::refresh_global_go_to_latest_shortcut(window.app_handle());
    }
    // Closing the main window quits the whole app (settings, debug, and
    // viewer windows included) — but only once the quit gate says so.
    // With work in flight the gate holds the exit, asks the user, and
    // runs its own countdown; the window must STAY OPEN for that, or
    // the dialog it's about to show goes with it. Nothing is torn down
    // on this path until the gate has waved the quit through, so a
    // "Keep working" leaves AI, MCP, and mDNS running. See `quit/`.
    if let tauri::WindowEvent::CloseRequested { api, .. } = event
        && window.label() == "main"
    {
        if quit::request_quit(window.app_handle()).is_held() {
            api.prevent_close();
        } else {
            stop_background_services();
            window.app_handle().exit(0);
        }
    }
    // Clean up app-wide resources only when the main window is destroyed
    if let tauri::WindowEvent::Destroyed = event
        && window.label() == "main"
    {
        stop_background_services();
    }
    // Free a viewer session when its window is destroyed. Closing a viewer
    // via the titlebar X never fires the FE `viewer_close` IPC (that only
    // runs from the in-app close path), so without this the `ViewerSession`
    // (backend, line index, watcher thread) leaked until app quit.
    // `close_session_for_window` is idempotent: if the FE already closed the
    // session via IPC, the lookup is a no-op.
    if let tauri::WindowEvent::Destroyed = event
        && window.label().starts_with("viewer-")
    {
        file_viewer::close_session_for_window(window.label());
    }
    // A Tags window owns its Multi-Rename session: however it goes (Escape, the red button, its
    // webview dying), the session is released and closed. Idempotent after the window's own close.
    if let tauri::WindowEvent::Destroyed = event {
        crate::tag_editor::window::end(window.label());
    }
}

/// Process-level signals: the first webview being ready, a reveal arriving from another
/// app, a quit being requested, and the process actually going away.
pub fn on_run_event(app: &AppHandle<Wry>, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::Ready => {
            // Install drag image detection swizzle. Needs a live webview to
            // discover wry's ObjC class, so it runs at Ready (not setup).
            #[cfg(target_os = "macos")]
            drag_image_detection::install(app.clone());
            // Teach the app delegate to answer `applicationDockMenu:`. Ready, because
            // tao has installed the delegate by then and AppKit only asks for the menu
            // on a right-click, so the timing is generous either way.
            #[cfg(target_os = "macos")]
            dock::menu::install(app.clone());
        }
        // Another app asked the OS to show a file, and the OS picked us
        // (`NSFileViewer`, see `reveal/`). It arrives as an open-documents
        // Apple Event, which AppKit hands to `application:openURLs:` and
        // Tauri surfaces here — cold launch and already-running alike, so
        // there is no argv path to cover. ⚠️ On a cold launch this fires
        // BEFORE `setup`, so the arm must not reach for managed state or the
        // logger. macOS only: no other platform emits this variant.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Opened { urls } => {
            reveal::on_urls_opened(app, urls);
        }
        // A Dock click (or a reopen from Finder). macOS apps bring back what the user can't
        // otherwise reach: a minimized main window, and a Tags window that's minimized or hidden
        // while its session is still held. AppKit would only activate the app.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => bring_windows_back(app),
        // ⌘Q, the app menu's Quit, the Dock's Quit, a logout or
        // restart, and every `AppHandle::exit` in the app all land
        // here. With non-instant work in flight the gate holds the
        // exit and takes the decision over (dialog + its own
        // countdown); with nothing running this is a pass-through and
        // the app quits exactly as it always did.
        //
        // A `restart()` carries `RESTART_EXIT_CODE`, for which Tauri
        // ignores `prevent_exit` outright — asking there would show a
        // dialog nobody could answer, so the gate never sees it.
        tauri::RunEvent::ExitRequested { ref api, code, .. } => {
            // An app alive enough to be asked to quit outlived any panic it wrote a crash
            // file about, so the next launch must not open with "Cmdr quit unexpectedly".
            // Free unless this session actually panicked. `crash_reporter/survival.rs`.
            crash_reporter::note_app_still_running();
            if code != Some(tauri::RESTART_EXIT_CODE) && quit::request_quit(app).is_held() {
                api.prevent_exit();
            }
        }
        tauri::RunEvent::Exit => {
            // Flush window geometry synchronously: the debounced writer
            // may have a pending change the process won't outlive. Not after
            // "Delete all Cmdr data": that would write a fresh state back.
            if !data_wipe::has_wiped() {
                window_state::save_on_exit(app);
            }

            // Stop any live search walk. Coverage stays honest either way
            // (a directory is marked listed only once its rows are
            // written, so a walk cut off mid-flight claims nothing it
            // didn't read), but a walk reading a disk for a window that
            // no longer exists is work nobody asked for.
            search::cancel_all_live_runs();

            // Restore ptpcamerad before exit so we don't leave the system
            // with the daemon disabled after Cmdr closes
            #[cfg(target_os = "macos")]
            if let Err(e) = mtp::macos_workaround::restore_ptpcamerad() {
                log::warn!("Failed to restore ptpcamerad on exit: {}", e);
            }

            stop_background_services();

            // Ends the open rejection episodes so their counts land, then waits briefly for the
            // file. After a wipe the writer writes nothing.
            security_log::on_exit();

            // Last, after the plugins' own Exit handlers (Tauri runs those first, and the store
            // plugin saves every open store there): remove what shutdown wrote back after a wipe.
            data_wipe::on_exit();
        }
        _ => {}
    }
}

/// Raises what [`windows_to_bring_back`] picks, main first, so a Tags window ends up in front.
#[cfg(target_os = "macos")]
fn bring_windows_back(app: &AppHandle<Wry>) {
    let windows = app.webview_windows();
    let states: Vec<ShownState<'_>> = windows
        .iter()
        .map(|(label, window)| ShownState {
            label,
            visible: window.is_visible().unwrap_or(true),
            minimized: window.is_minimized().unwrap_or(false),
        })
        .collect();
    let is_held = |session_id: &str| crate::multi_rename::session::is_held(session_id);
    for label in windows_to_bring_back(&states, is_held) {
        crate::commands::window_ordering::raise_child_window(app.clone(), label.to_owned());
    }
}

/// One window as a Dock click sees it.
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "only macOS has a Dock reopen; the decision is still pinned by tests"
    )
)]
pub(crate) struct ShownState<'a> {
    label: &'a str,
    visible: bool,
    minimized: bool,
}

#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "only macOS has a Dock reopen; the decision is still pinned by tests"
    )
)]
/// The windows a Dock click brings back, in raise order: main if minimized, then every Tags window
/// that's minimized or hidden while `is_held` still holds its session.
fn windows_to_bring_back<'a>(windows: &[ShownState<'a>], is_held: impl Fn(&str) -> bool) -> Vec<&'a str> {
    let out_of_reach = |w: &&ShownState<'a>| w.minimized || !w.visible;
    // A hidden main window is still starting up (it shows itself once its page is ready).
    let main = windows.iter().filter(|w| w.label == "main" && w.minimized);
    // A released Tags window is on its way out; showing it would put a half-torn-down page up.
    let tags = windows
        .iter()
        .filter(out_of_reach)
        .filter(|w| crate::tag_editor::window::session_of_label(w.label).is_some_and(&is_held));
    main.chain(tags).map(|w| w.label).collect()
}

#[cfg(test)]
mod tests {
    use super::{ShownState, windows_to_bring_back};

    fn window(label: &str, visible: bool, minimized: bool) -> ShownState<'_> {
        ShownState {
            label,
            visible,
            minimized,
        }
    }

    /// A Dock click brings back what the user can't otherwise reach: a minimized main window, and a
    /// Tags window that's minimized or ordered out while its session is still held. Main comes
    /// first, so a Tags window ends up in front of it.
    #[test]
    fn a_dock_click_brings_back_minimized_main_and_hidden_or_minimized_held_tags_windows() {
        let held = |id: &str| id != "gone";
        let windows = [
            window("tags-hidden", false, false),
            window("main", true, true),
            window("tags-minimized", true, true),
            window("tags-shown", true, false),
            window("settings", false, false),
        ];
        assert_eq!(
            windows_to_bring_back(&windows, held),
            vec!["main", "tags-hidden", "tags-minimized"]
        );
    }

    /// A Tags window whose session is released is on its way out (or an orphan the next open takes
    /// down): showing it again would put a half-torn-down window up. And a main window that's only
    /// hidden is still starting up, which shows it itself once its page is ready.
    #[test]
    fn a_dock_click_leaves_released_tags_windows_and_a_starting_main_window_alone() {
        let held = |id: &str| id != "gone";
        let windows = [window("main", false, false), window("tags-gone", false, false)];
        assert!(windows_to_bring_back(&windows, held).is_empty());
    }
}
