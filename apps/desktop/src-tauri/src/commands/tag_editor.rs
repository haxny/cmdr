//! IPC for the Tags tab of the Multi-Rename Tool. Thin: the work is `crate::tag_editor`.
//! File work runs on the blocking pool, never on the IPC thread.

use cmdr_tags::shared::CoverEdit;
use cmdr_tags::tools::TagTool;
use cmdr_tags::{ContentKind, NamePattern, RestoreReport};

use std::time::Instant;

use crate::multi_rename::session::files_of;
use crate::tag_editor::activity;
use crate::tag_editor::apply::{ApplyRequest, TagApplyReport};
use crate::tag_editor::cover::{CoverCandidate, candidates_in, read_cover, thumbnail_url};
use crate::tag_editor::playback::{audio_url, open_in_player, player_targets, release};
use crate::tag_editor::session::{
    FileChangeView, LOCAL_VOLUME_ID, NamePreviewRow, TagEdit, TagEditorError, changes, close, edit, open,
    preview_names, shared_cover, tool as run_tool,
};
use crate::tag_editor::text_tools::{TagTextTool, run as run_text_tool};
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
    blocking(move || {
        let started = Instant::now();
        let result = open(&session_id, restart);
        activity::opened(&result, started);
        result
    })
    .await
}

/// Applies edits in memory; an unknown track refuses the whole batch.
#[tauri::command]
#[specta::specta]
pub async fn edit_tags(session_id: String, edits: Vec<TagEdit>) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = edit(&session_id, edits);
        activity::edited("edit", &result, started);
        result
    })
    .await
}

/// Runs a tool over the tracks at `scope` (their paths), or over the whole session for `None`.
#[tauri::command]
#[specta::specta]
pub async fn run_tag_tool(
    session_id: String,
    tool: TagTool,
    scope: Option<Vec<String>>,
) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = run_tool(&session_id, tool, scope);
        activity::edited("tool", &result, started);
        result
    })
    .await
}

/// Runs find/replace or a field-from-mask format over the tracks at `scope`, or the whole session
/// for `None`.
#[tauri::command]
#[specta::specta]
pub async fn run_tag_text_tool(
    session_id: String,
    tool: TagTextTool,
    scope: Option<Vec<String>>,
) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = run_text_tool(&session_id, tool, scope);
        activity::edited("text tool", &result, started);
        result
    })
    .await
}

/// What a file-name pattern reads from each track's name at `scope` (or every track), changing nothing.
#[tauri::command]
#[specta::specta]
pub async fn preview_tags_from_names(
    session_id: String,
    pattern: NamePattern,
    scope: Option<Vec<String>>,
) -> Result<Vec<NamePreviewRow>, TagEditorError> {
    blocking(move || preview_names(&session_id, pattern, scope)).await
}

/// What a write as `kind` would change, per file, including what the kind removes.
#[tauri::command]
#[specta::specta]
pub async fn get_tag_changes(session_id: String, kind: ContentKind) -> Result<Vec<FileChangeView>, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = changes(&session_id, kind);
        activity::listed(&result, started);
        result
    })
    .await
}

/// Writes the changed files.
#[tauri::command]
#[specta::specta]
pub async fn apply_tags(
    app: tauri::AppHandle,
    session_id: String,
    request: ApplyRequest,
) -> Result<TagApplyReport, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = crate::tag_editor::apply::apply(&app, &session_id, request);
        activity::applied(&result, started);
        result
    })
    .await
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
    blocking(move || {
        let started = Instant::now();
        let result = crate::tag_editor::undo::undo(&app, apply_id, session_id);
        activity::undone(&result, started);
        result
    })
    .await
}

/// Ends the tag session, dropping its edits.
#[tauri::command]
#[specta::specta]
pub async fn close_tag_editor(session_id: String) {
    close(&session_id);
}

/// A `cmdr-media://localhost/<token>` URL that plays the track at `path` in an `<audio>` element.
/// Only a readable track of the session gets one (`UnknownTrack` otherwise).
#[tauri::command]
#[specta::specta]
pub async fn tag_track_audio_url(session_id: String, path: String) -> Result<String, TagEditorError> {
    blocking(move || audio_url(&session_id, &path)).await
}

/// Ends a URL `tag_track_audio_url` gave. Closing the tag session ends all of them.
#[tauri::command]
#[specta::specta]
pub async fn release_tag_track_audio(url: String) {
    release(&url);
}

/// Opens tracks of the session in foobar2000 when it's installed, else each in its default app.
/// Returns once the launch is asked for, not when the player is up.
#[tauri::command]
#[specta::specta]
pub async fn open_tags_in_player(
    app: tauri::AppHandle,
    session_id: String,
    paths: Vec<String>,
) -> Result<(), TagEditorError> {
    blocking(move || open_in_player(&app, player_targets(&session_id, &paths)?)).await
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
        let started = Instant::now();
        let result = read_cover(&local_folder(&session_id)?, &file_name).and_then(|cover| {
            edit(
                &session_id,
                vec![TagEdit::Cover {
                    edit: CoverEdit::Set(cover),
                }],
            )
        });
        activity::edited("cover from file", &result, started);
        result
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
