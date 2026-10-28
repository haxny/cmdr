//! Tauri commands for the in-session child-window position cache, and the one safe way for a
//! child window to close itself. See `crate::child_window_state` for the position-cache design.

use tauri::{AppHandle, Manager, Runtime, State};

use crate::child_window_state::{ChildWindowRect, ChildWindowRectStore};

/// How long a child window stays hidden before it's destroyed.
///
/// Destroying a content-heavy webview while a layer-tree commit from its web content process is
/// still in flight makes WebKit's UI-side `RemoteLayerTreeDrawingAreaProxy::commitLayerTree`
/// dereference freed state and take the whole app down with a SIGSEGV. Hiding first pulls the view
/// out of the compositor so no NEW commits are produced, and the wait lets in-flight ones drain.
///
/// Measured with a repro harness driving real Escape keypresses (macOS 26.5.2, 2026-07-23): at 0 ms
/// the app crashed after 36 close cycles; at 100 ms it survived 80 consecutive cycles clean. Treat
/// it as a floor, ❌ never lower it. `docs/notes/child-window-close-webkit-crash.md`.
///
/// On Linux the same delay covers a different problem: destroying the webview on the same GTK
/// main-loop tick that handled the key stalls IPC queued behind it from other webviews.
const WINDOW_CLOSE_DEFER_MS: u64 = 100;

/// Hide a child window, then destroy it a moment later.
///
/// ❗ This runs in RUST rather than in the window's own webview on purpose. WebKit throttles timers
/// in a hidden page to roughly 1 Hz, so a `setTimeout` inside a webview that just hid itself can
/// slip by a second or more, leaving an invisible window alive and still holding a web content
/// process. A Rust timer can't be throttled by the page it's about to close. It also means the
/// windows need no `core:window:allow-hide` in their capabilities, since nothing calls `hide` from
/// JavaScript.
///
/// The destroy is unconditional once scheduled: an early return between the hide and the destroy
/// (or a close the page could veto) would leak exactly the invisible-but-alive window this is
/// meant to prevent.
#[tauri::command]
#[specta::specta]
pub fn close_child_window(app: AppHandle, label: String) {
    hide_then_destroy(&app, &label);
}

/// [`close_child_window`]'s body, generic so the mock runtime can drive it, and the way any other
/// Rust path that has decided a child window goes (the Tags window's orphan sweep) takes it down.
pub(crate) fn hide_then_destroy<R: Runtime>(app: &AppHandle<R>, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        // Already gone: a double-close (Escape plus the red button) is ordinary, not a fault.
        // allowed-log-private: a Tauri window label of our own
        log::debug!("Child window close: no window labelled {label}, nothing to close");
        return;
    };

    // macOS only. `orderOut:` is what stops new layer-tree commits, and the crash it defends
    // against is a macOS WebKit one; on Linux the defer alone is the whole fix, and hiding there
    // would be an untested change to how webkit2gtk tears a window down.
    #[cfg(target_os = "macos")]
    if let Err(e) = window.hide() {
        // Not fatal: without the hide we're back to the old behavior, which the delay alone made
        // rare. Closing anyway beats leaving the window up.
        // allowed-log-private: a Tauri window label of our own
        log::warn!("Child window close: couldn't hide {label} before closing: {e}");
    }

    let app = app.clone();
    let label = label.to_owned();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(WINDOW_CLOSE_DEFER_MS)).await;
        // The handle taken before the wait, ❌ never the label looked up again: a window opened
        // under the same label during the wait (Escape, the red button, ⌘, again, all inside
        // 100 ms) is a new one, and must not go. Another close (Escape plus the red button, the
        // Tags orphan sweep) may have taken this one during the wait: then it's gone already.
        // (Tauri sends `destroy` to the handle's window id, not its label: verified on
        // tauri-runtime-wry 2.12.2, `WindowMessage::Destroy` in its source, 2026-10-29.)
        if app.get_webview_window(&label).is_none() {
            return;
        }
        // ❗ `destroy`, not `close`: every caller has already decided, and `close` would put the
        // decision to the page again. A JS `onCloseRequested` listener (the Tags window's) makes
        // Tauri veto the close and hand it to a page that is now HIDDEN, which left the window
        // invisible and alive for good. `Destroyed` still fires, so every cleanup on it runs (the
        // viewer's session, the Tags window's `end`); no child window does cleanup on
        // `CloseRequested` (only `main` does, and it never comes here).
        if let Err(e) = window.destroy() {
            // Gone during the wait, with a new window under its label: ordinary, not a fault.
            // allowed-log-private: a Tauri window label of our own
            log::debug!("Child window close: couldn't destroy {label}: {e}");
        }
    });
}

/// Returns the saved rect for a child window label, or `None` if no entry
/// exists (first open in this session, or never opened).
#[tauri::command]
#[specta::specta]
pub fn get_child_window_rect(label: String, store: State<'_, ChildWindowRectStore>) -> Option<ChildWindowRect> {
    store.get(&label)
}

/// Saves the rect for a child window label. Called from the frontend's
/// move/resize listeners on Settings and Debug.
#[tauri::command]
#[specta::specta]
pub fn set_child_window_rect(label: String, rect: ChildWindowRect, store: State<'_, ChildWindowRectStore>) {
    store.set(label, rect);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::{RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

    use super::hide_then_destroy;

    /// A close that's been decided must not be up for a vote again. The Tags page's JS close
    /// listener vetoes every `CloseRequested` it gets (Tauri hands the close to it and waits), so a
    /// `close()` after the hide left the window invisible and alive for good (the "⌘Tab lost my
    /// Tags window" bug). A listener that never answers stands in for it here: the window still
    /// goes, which the mock run loop shows by ending once its only window is gone.
    #[test]
    fn a_window_whose_close_listener_never_answers_is_still_destroyed() {
        let (gone_tx, gone_rx) = mpsc::channel();
        // The mock run loop blocks its thread until the last window goes, so it gets a thread of
        // its own and the test waits on it with a deadline instead of hanging. On a failure that
        // thread stays blocked; nextest runs each test in its own process, which ends it.
        std::thread::spawn(move || {
            let app = mock_builder().build(mock_context(noop_assets())).expect("a mock app");
            WebviewWindowBuilder::new(&app, "tags-s1", WebviewUrl::default())
                .build()
                .expect("a mock Tags window");
            // The mock runtime hands window events to the run callback only, so the veto lives here.
            app.run_return(|handle, event| match event {
                RunEvent::Ready => hide_then_destroy(handle, "tags-s1"),
                RunEvent::WindowEvent {
                    event: WindowEvent::CloseRequested { api, .. },
                    ..
                } => api.prevent_close(),
                _ => {}
            });
            let _ = gone_tx.send(());
        });
        assert!(
            gone_rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "the window survived its close: a vetoed close() left it alive"
        );
    }
}
