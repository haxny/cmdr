//! IPC for the Tags tab of the Multi-Rename Tool. Thin: the work is `crate::tag_editor`.
//! File work runs on the blocking pool, never on the IPC thread.

use cmdr_tags::RestoreReport;
use cmdr_tags::shared::CoverEdit;
use cmdr_tags::tools::TagTool;

use crate::multi_rename::session::files_of;
use crate::tag_editor::apply::{ApplyRequest, TagApplyReport};
use crate::tag_editor::cover::{CoverCandidate, candidates_in, read_cover, thumbnail_url};
use crate::tag_editor::session::{
    FileChangeView, LOCAL_VOLUME_ID, TagEdit, TagEditorError, changes, close, edit, open, shared_cover,
    tool as run_tool,
};
use crate::tag_editor::view::TagEditorView;

/// Runs blocking `work` off the IPC thread.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, TagEditorError> + Send + 'static,
) -> Result<T, TagEditorError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| TagEditorError::TaskFailed)?
}

/// Starts (or, when `restart` is set, restarts) the tag session of a Multi-Rename session.
#[tauri::command]
#[specta::specta]
pub async fn open_tag_editor(session_id: String, restart: bool) -> Result<TagEditorView, TagEditorError> {
    blocking(move || open(&session_id, restart)).await
}

/// Applies edits in memory; an unknown track refuses the whole batch.
#[tauri::command]
#[specta::specta]
pub async fn edit_tags(session_id: String, edits: Vec<TagEdit>) -> Result<TagEditorView, TagEditorError> {
    blocking(move || edit(&session_id, edits)).await
}

/// Runs a clean-up tool over the whole session.
#[tauri::command]
#[specta::specta]
pub async fn run_tag_tool(session_id: String, tool: TagTool) -> Result<TagEditorView, TagEditorError> {
    blocking(move || run_tool(&session_id, tool)).await
}

/// What a write would change, per file.
#[tauri::command]
#[specta::specta]
pub async fn get_tag_changes(session_id: String) -> Result<Vec<FileChangeView>, TagEditorError> {
    blocking(move || changes(&session_id)).await
}

/// Writes the changed files.
#[tauri::command]
#[specta::specta]
pub async fn apply_tags(
    app: tauri::AppHandle,
    session_id: String,
    request: ApplyRequest,
) -> Result<TagApplyReport, TagEditorError> {
    blocking(move || crate::tag_editor::apply::apply(&app, &session_id, request)).await
}

/// Writes the old tags of an apply back. `session_id` is the sheet asking: its tag session takes
/// the restored files as read, edits to them dropped (its user confirmed that); any other keeps
/// its unwritten edits.
#[tauri::command]
#[specta::specta]
pub async fn undo_tag_apply(
    app: tauri::AppHandle,
    apply_id: String,
    session_id: Option<String>,
) -> Result<RestoreReport, TagEditorError> {
    blocking(move || crate::tag_editor::undo::undo(&app, apply_id, session_id)).await
}

/// Ends the tag session, dropping its edits.
#[tauri::command]
#[specta::specta]
pub async fn close_tag_editor(session_id: String) {
    close(&session_id);
}

/// The session's folder, when its files are on the local disk.
fn local_folder(session_id: &str) -> Result<std::path::PathBuf, TagEditorError> {
    let files = files_of(session_id).map_err(|_| TagEditorError::NoSession)?;
    if files.volume_id == LOCAL_VOLUME_ID {
        Ok(files.dir)
    } else {
        Err(TagEditorError::ReadOnly)
    }
}

/// The images of the session's folder that can be a cover.
#[tauri::command]
#[specta::specta]
pub async fn list_cover_candidates(session_id: String) -> Result<Vec<CoverCandidate>, TagEditorError> {
    blocking(move || candidates_in(&local_folder(&session_id)?)).await
}

/// Gives every file the cover image `file_name` of the session's folder.
#[tauri::command]
#[specta::specta]
pub async fn set_cover_from_file(session_id: String, file_name: String) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let cover = read_cover(&local_folder(&session_id)?, &file_name)?;
        edit(
            &session_id,
            vec![TagEdit::Cover {
                edit: CoverEdit::Set(cover),
            }],
        )
    })
    .await
}

/// The cover every file shares, as a `data:` URL for the thumbnail, or `None` when there is none,
/// the files differ, or it isn't a JPEG, PNG, or WebP image.
#[tauri::command]
#[specta::specta]
pub async fn get_cover_image(session_id: String) -> Result<Option<String>, TagEditorError> {
    blocking(move || Ok(shared_cover(&session_id)?.and_then(|cover| thumbnail_url(&cover.data)))).await
}
