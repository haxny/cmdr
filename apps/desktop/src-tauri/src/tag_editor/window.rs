//! The Tags window's hold on its Multi-Rename session. The main window opens a fresh session for
//! each window (`tags-<sessionId>`) and holds it here, so neither the idle limit nor the cap
//! evicts it; the window going ends it, whichever way it goes (Escape, the red button, a crash
//! of its webview), because `app_lifecycle` calls [`end`] on the window's `Destroyed` too.

use serde::{Deserialize, Serialize};

use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::session::{self as rename_session, files_of};

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

/// Holds `session_id` for its window and says what it holds.
pub(crate) fn hold(session_id: &str) -> Result<TagsSessionInfo, MultiRenameError> {
    rename_session::hold(session_id)?;
    let files = files_of(session_id)?;
    let tracks = files
        .names
        .iter()
        .filter(|name| cmdr_tags::format_of(name).is_some())
        .count();
    let folder_name = files
        .dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| files.dir.to_string_lossy().into_owned());
    Ok(TagsSessionInfo { tracks, folder_name })
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
