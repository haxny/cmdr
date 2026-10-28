//! The Tags window's hold on its Multi-Rename session. The main window opens a fresh session for
//! each window (`tags-<sessionId>`) and holds it here, so neither the idle limit nor the cap
//! evicts it; the window going ends it, whichever way it goes (Escape, the red button, a crash
//! of its webview), because `app_lifecycle` calls [`end`] on the window's `Destroyed` too.

use serde::{Deserialize, Serialize};

use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::session::{self as rename_session, files_of};
use crate::window_events::TagsClipboardAction;

/// The label prefix of a Tags window; the rest of the label is its session's id.
const LABEL_PREFIX: &str = "tags-";

/// What the main window needs to open a held session's window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagsSessionInfo {
    /// How many of the session's files are audio the engine can tag (by name; none means no window).
    pub tracks: usize,
    /// The session's folder, for the window's title.
    pub folder_name: String,
    /// The session of a Tags window already showing the same files: the main window raises it
    /// and gives this session back, so one book never has two windows.
    pub already_open: Option<String>,
}

/// The session a window label names: `Some` only for a Tags window.
pub(crate) fn session_of_label(label: &str) -> Option<&str> {
    label.strip_prefix(LABEL_PREFIX).filter(|id| !id.is_empty())
}

/// The focused Tags window's label among `(label, focused)` pairs: where Edit > Select all (⌘A)
/// goes, since its accelerator fires before the page sees the key (`menu_handlers.rs`).
pub(crate) fn focused_tags_window<'a>(windows: impl IntoIterator<Item = (&'a str, bool)>) -> Option<&'a str> {
    windows
        .into_iter()
        .find(|(label, focused)| *focused && session_of_label(label).is_some())
        .map(|(label, _)| label)
}

/// What a native Edit / Select-menu item asks of a focused Tags window (`menu_handlers.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagsMenuSelection {
    /// Edit > Select all (⌘A): every track, or the text of the box being typed in.
    SelectAll,
    /// Select > Deselect all (⌘⇧A): no track selected.
    DeselectAll,
    /// Edit > Cut, Copy or Paste (⌘X, ⌘C, ⌘V): the field under the cursor, or the text box's own.
    Clipboard(TagsClipboardAction),
}

/// The Tags window's answer to menu item `id`, or `None` for an item it doesn't take. Their
/// accelerators fire before the page sees the key, so this is the keys' only road there.
pub(crate) fn menu_selection_of(id: &str) -> Option<TagsMenuSelection> {
    use crate::menu::{DESELECT_ALL_ID, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, SELECT_ALL_ID};
    match id {
        SELECT_ALL_ID => Some(TagsMenuSelection::SelectAll),
        DESELECT_ALL_ID => Some(TagsMenuSelection::DeselectAll),
        EDIT_CUT_ID => Some(TagsMenuSelection::Clipboard(TagsClipboardAction::Cut)),
        EDIT_COPY_ID => Some(TagsMenuSelection::Clipboard(TagsClipboardAction::Copy)),
        EDIT_PASTE_ID => Some(TagsMenuSelection::Clipboard(TagsClipboardAction::Paste)),
        _ => None,
    }
}

/// Whether window `label` may hand an edit action back (`tags_native_edit`): a Tags window that
/// still has focus. The action goes to the key window's first responder, so any other caller (or a
/// Tags window the user switched away from) would cut, copy or paste somewhere else.
pub(crate) fn may_hand_back(label: &str, focused: bool) -> bool {
    focused && session_of_label(label).is_some()
}

/// The Edit item whose native action (`cut:`, `copy:`, `paste:`) `action` hands back to the
/// window's first responder, for a text box or the cover section (`tags_native_edit`).
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the responder chain is macOS-only; the mapping is still pinned by tests"
    )
)]
pub(crate) fn native_edit_id_of(action: TagsClipboardAction) -> &'static str {
    match action {
        TagsClipboardAction::Cut => crate::menu::EDIT_CUT_ID,
        TagsClipboardAction::Copy => crate::menu::EDIT_COPY_ID,
        TagsClipboardAction::Paste => crate::menu::EDIT_PASTE_ID,
    }
}

/// The `Tags window` command a main-menu item's key runs while a Tags window is focused, or `None`
/// for an item the window doesn't take.
///
/// The Tags window installs the main menu (greyed), whose File / Select items register F2–F8,
/// ⇧F2, ⇧F4 and ⇧F8. On macOS a bare-key accelerator fires ahead of the page, and the file-scoped
/// focus guard would drop it, so `menu_handlers.rs` sends the press here instead (`TagsMenuKey`).
/// Each item maps to the Tags command holding the same default key; the window checks the item
/// still has that key before acting, so a rebound file command never runs a tag tool.
///
/// ❌ Not View > Switch pane's Tab: the page sees Tab anyway (main's dialogs move focus with it
/// behind the same accelerator), and a routed Tab would switch panes where Tab must move focus.
pub(crate) fn menu_key_command_of(id: &str) -> Option<&'static str> {
    use crate::menu::{
        COMPARE_DIRECTORIES_ID, EDIT_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID,
        FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, RENAME_ID,
    };
    match id {
        RENAME_ID => Some("tags.edit"),
        COMPARE_DIRECTORIES_ID => Some("tags.editNumber"),
        FILE_VIEW_ID => Some("tags.play"),
        EDIT_ID => Some("tags.replace"),
        FILE_NEW_FILE_ID => Some("tags.case"),
        FILE_COPY_ID => Some("tags.format"),
        FILE_MOVE_ID => Some("tags.fromName"),
        FILE_NEW_FOLDER_ID => Some("tags.number"),
        FILE_DELETE_ID => Some("tags.clearField"),
        FILE_DELETE_PERMANENTLY_ID => Some("tags.removeExtras"),
        _ => None,
    }
}

/// Holds `session_id` for its window and says what it holds, and which window already shows the
/// same files, if one does.
pub(crate) fn hold(session_id: &str) -> Result<TagsSessionInfo, MultiRenameError> {
    rename_session::hold(session_id)?;
    let files = files_of(session_id)?;
    let tracks = files
        .names
        .iter()
        .filter(|name| cmdr_tags::format_of(name).is_some())
        .count();
    // The folder the tracks share, so a book opened from its parent is named after the book.
    let folder = super::cover::tracks_folder(&files.dir, files.names.iter());
    let folder_name = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.to_string_lossy().into_owned());
    Ok(TagsSessionInfo {
        tracks,
        folder_name,
        already_open: rename_session::held_twin(session_id),
    })
}

/// Releases and closes the session of the Tags window `label` (its tag session goes with it).
/// No-op for any other window, and for a session already gone.
pub(crate) fn end(label: &str) {
    let Some(session_id) = session_of_label(label) else {
        return;
    };
    rename_session::release(session_id);
    rename_session::close(session_id);
}

/// Takes down every Tags window whose session is no longer held: closed (its `end` released it)
/// but somehow still there. Opening a Tags window calls it (`hold_tags_session`), so an orphan
/// can't sit hidden for the life of the process, holding its web content process. Through the
/// same hide-and-wait as any child-window close, since its page may still be committing layers.
pub(crate) fn close_orphans<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let labels: Vec<String> = app.webview_windows().into_keys().collect();
    for label in orphaned_tags_windows(labels.iter().map(String::as_str), rename_session::is_held) {
        log::info!(target: "tag_editor", "Closing a Tags window whose session was already released");
        crate::commands::child_window_state::hide_then_destroy(app, label);
    }
}

/// The Tags windows among `labels` whose session `is_held` no longer holds.
pub(crate) fn orphaned_tags_windows<'a>(
    labels: impl IntoIterator<Item = &'a str>,
    is_held: impl Fn(&str) -> bool,
) -> Vec<&'a str> {
    labels
        .into_iter()
        .filter(|label| session_of_label(label).is_some_and(|id| !is_held(id)))
        .collect()
}

/// Whether the Tags window of `session_id` may close without the discard question: its session
/// holds no unwritten edit and no tag write or undo is running (any session's: the counter isn't
/// per session, and waiting out someone else's write costs nothing). Its error boundary asks
/// before it closes the window, since the edits live here and a closed window ends the session.
pub(crate) fn may_close_unasked(session_id: &str) -> bool {
    !super::session::has_unwritten(session_id) && !super::at_quit::is_writing()
}
