//! The main-menu events a focused Tags window takes (`menu/menu_handlers.rs` asks first): Edit >
//! Select all (⌘A), Select > Deselect all (⌘⇧A), and the File / Select F-keys. Each one's
//! accelerator fires before the page sees the key, and the file-scoped focus guard after this
//! would drop it, so this is the keys' only road there. What each item means is `window.rs`'s.

use tauri::{AppHandle, Manager, Wry};
use tauri_specta::Event as _;

use super::window::{TagsMenuSelection, focused_tags_window, menu_key_command_of, menu_selection_of};
use crate::window_events::{TagsDeselectAll, TagsMenuKey, TagsSelectAll};

/// Sends menu item `id` to the focused Tags window when it takes it, and answers whether it did
/// (then the item is handled). Main focused: no Tags window is, and the item runs as before.
pub(crate) fn route_menu_event(app: &AppHandle<Wry>, id: &str) -> bool {
    let selection = menu_selection_of(id);
    let key_command = menu_key_command_of(id);
    if selection.is_none() && key_command.is_none() {
        return false;
    }
    let Some(label) = focused_tags_label(app) else {
        return false;
    };
    let sent = match (selection, key_command) {
        (Some(TagsMenuSelection::SelectAll), _) => TagsSelectAll.emit_to(app, label.as_str()),
        (Some(TagsMenuSelection::DeselectAll), _) => TagsDeselectAll.emit_to(app, label.as_str()),
        (None, Some(command_id)) => {
            // The window acts only while the item still holds the Tags command's key.
            let menu_command_id = crate::menu::menu_id_to_command(id)
                .map(|(command, _)| command)
                .unwrap_or_default();
            TagsMenuKey {
                command_id: command_id.to_string(),
                menu_command_id: menu_command_id.to_string(),
            }
            .emit_to(app, label.as_str())
        }
        (None, None) => return false,
    };
    if let Err(e) = sent {
        log::warn!(target: "menu", "failed to send a menu key to a Tags window: {e}");
    }
    true
}

/// The label of the Tags window that has focus, if one does.
fn focused_tags_label(app: &AppHandle<Wry>) -> Option<String> {
    let windows = app.webview_windows();
    focused_tags_window(
        windows
            .iter()
            .map(|(label, window)| (label.as_str(), window.is_focused().unwrap_or(false))),
    )
    .map(str::to_string)
}
