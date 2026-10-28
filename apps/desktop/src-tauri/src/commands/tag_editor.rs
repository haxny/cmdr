//! IPC for the Tags tab of the Multi-Rename Tool. Thin: the work is `crate::tag_editor`.
//! File work runs on the blocking pool, never on the IPC thread.

use cmdr_tags::tools::TagTool;
use cmdr_tags::{ContentKind, NamePattern, RestoreReport};

use std::time::Instant;

use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::session::MultiRenameOpened;
use crate::multi_rename::session::files_of;
use crate::tag_editor::activity;
use crate::tag_editor::apply::{ApplyRequest, TagApplyReport};
use crate::tag_editor::cover::{CoverCandidate, candidates_in, cover_from_bytes, read_cover, read_dropped};
use crate::tag_editor::cover_choices::{
    CoverChoice, image_of, list as list_covers, set_for_all, set_from_path_within, thumbnail, use_for_all,
};
use crate::tag_editor::playback::{audio_url, open_in_player, player_targets, release};
use crate::tag_editor::session::{
    FileChangeView, LOCAL_VOLUME_ID, NamePreviewRow, TagEdit, TagEditorError, changes, close, edit, open,
    preview_names, tool as run_tool,
};
use crate::tag_editor::text_tools::{TagTextTool, run as run_text_tool};
use crate::tag_editor::view::TagEditorView;
use crate::tag_editor::window::TagsSessionInfo;

/// How long a dropped picture's read may take: the read tier of `commands/CLAUDE.md` (2 s).
const COVER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// How long a thumbnail's decode and resize may take (CPU, growing with the picture).
const THUMBNAIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

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
        let result =
            read_cover(&local_folder(&session_id)?, &file_name).and_then(|cover| set_for_all(&session_id, cover));
        activity::edited("cover from file", &result, started);
        result
    })
    .await
}

/// A cover of the files as a `data:` URL for a thumbnail or the large view: the one named by
/// `key` (a `CoverChoice::key`; `"none"` has no image), or for `None` the cover every file
/// shares. `None` comes back when there is none, the files differ, or it isn't a JPEG, PNG, or
/// WebP image; a key no file carries any more is `UnknownCover`.
#[tauri::command]
#[specta::specta]
pub async fn get_cover_image(session_id: String, key: Option<String>) -> Result<Option<String>, TagEditorError> {
    blocking(move || image_of(&session_id, key.as_deref())).await
}

/// Every distinct cover of the session's readable files with how many carry it, the most common
/// first, and the files without one last (`key: "none"`).
#[tauri::command]
#[specta::specta]
pub async fn list_tag_covers(session_id: String) -> Result<Vec<CoverChoice>, TagEditorError> {
    blocking(move || list_covers(&session_id)).await
}

/// Gives every file the cover `key` names, in memory: a change listed for Write, nothing written.
#[tauri::command]
#[specta::specta]
pub async fn use_cover_for_all(session_id: String, key: String) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = use_for_all(&session_id, &key);
        activity::edited("cover for all", &result, started);
        result
    })
    .await
}

/// Gives every file a pasted picture as its cover, in memory. The bytes must be a JPEG, PNG, or
/// WebP image by their first bytes (`CoverNotAnImage`) of at most 10 MiB (`CoverTooLarge`).
#[tauri::command]
#[specta::specta]
pub async fn set_cover_from_bytes(session_id: String, data: Vec<u8>) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = cover_from_bytes(data).and_then(|cover| set_for_all(&session_id, cover));
        activity::edited("cover from paste", &result, started);
        result
    })
    .await
}

/// Gives every file a dropped picture file as its cover, in memory. Only a regular file is read
/// (a folder, FIFO, or symlink is `CoverNotAFile`), with the same type sniff and size cap. A drop
/// can come from a network mount: the read gets the read tier's deadline (`COVER_READ_TIMEOUT`),
/// past it the answer is `CoverTimedOut` and nothing changes, then or later.
#[tauri::command]
#[specta::specta]
pub async fn set_cover_from_path(session_id: String, path: String) -> Result<TagEditorView, TagEditorError> {
    let started = Instant::now();
    let result = set_from_path_within(session_id, path.into(), COVER_READ_TIMEOUT, read_dropped).await;
    activity::edited("cover from drop", &result, started);
    result
}

/// A small PNG `data:` URL of the cover `key` names, for a tile (about twice its size, cached by
/// key); `None` for "none" or a cover that doesn't decode. The viewer keeps the full image
/// (`get_cover_image`).
#[tauri::command]
#[specta::specta]
pub async fn get_cover_thumbnail(session_id: String, key: String) -> Result<Option<String>, TagEditorError> {
    cmdr_fs::deadline::blocking_typed_result_with_timeout(
        THUMBNAIL_TIMEOUT,
        || TagEditorError::CoverTimedOut,
        |_| TagEditorError::TaskFailed,
        move || thumbnail(&session_id, &key),
    )
    .await
}

/// Holds the Multi-Rename session a Tags window will own, so no eviction takes it, and says how
/// many of its files are tracks and what its folder is called. The main window calls it right
/// after opening the session; the window's close (or its `Destroyed`) releases it.
#[tauri::command]
#[specta::specta]
pub async fn hold_tags_session(session_id: String) -> Result<TagsSessionInfo, MultiRenameError> {
    crate::tag_editor::window::hold(&session_id)
}

/// Lets eviction take the session again. No-op when it's already gone.
#[tauri::command]
#[specta::specta]
pub async fn release_tags_session(session_id: String) {
    crate::multi_rename::session::release(&session_id);
}

/// A new Multi-Rename session over the files of `session_id` (an open sheet's), in their current
/// order, for a Tags window: the sheet closing never ends the window's session.
#[tauri::command]
#[specta::specta]
pub async fn reopen_for_tags(session_id: String) -> Result<MultiRenameOpened, MultiRenameError> {
    crate::multi_rename::session::reopen(&session_id)
}

/// Opens a Multi-Rename session over the pane's selection for a Tags window, as
/// `open_multi_rename` does, but with an empty media cache: no EXIF or tag read is queued, since
/// the window reads the tags itself.
#[tauri::command]
#[specta::specta]
pub async fn open_tags_session(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    expected_sequence: u64,
) -> Result<MultiRenameOpened, MultiRenameError> {
    cmdr_fs::deadline::blocking_typed_result_with_timeout(
        std::time::Duration::from_secs(2),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || {
            crate::multi_rename::session::open_for_tags(
                &listing_id,
                include_hidden,
                selected_indices.as_deref(),
                expected_sequence,
            )
        },
    )
    .await
}
