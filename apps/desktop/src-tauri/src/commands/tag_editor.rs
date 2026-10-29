//! IPC for the Tags window. Thin: the work is `crate::tag_editor`.
//! File work runs on the blocking pool, never on the IPC thread.

use cmdr_tags::tools::TagTool;
use cmdr_tags::{ContentKind, NamePattern, RestoreReport};

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use cmdr_fs::deadline::{SESSION_IO_TIMEOUT, StallWatch, blocking_typed_result_until_stalled};

use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::session::files_of;
use crate::multi_rename::session::{MultiRenameOpened, OpenHandshake, TagsOpened, plan_for_tags};
use crate::tag_editor::activity;
use crate::tag_editor::apply::{ApplyRequest, TagApplyReport};
use crate::tag_editor::at_quit;
use crate::tag_editor::cover::{CoverCandidate, candidates_in, read_cover, read_dropped_counted};
use crate::tag_editor::cover_choices::{
    CoverChoice, DROP_READS, image_of, list as list_covers, set_for_all, set_from_bytes_within, set_from_path_within,
    thumbnail, use_for_all,
};
use crate::tag_editor::history::{redo_edit, undo_edit};
use crate::tag_editor::last_apply::{LastTagApply, last_of};
use crate::tag_editor::playback::{
    AppChoice, PlayerOpened, audio_url, choose_player, open_in_player, player_choices, player_targets, release,
};
use crate::tag_editor::replace_presets::{
    MAX_PRESETS, REPLACE_PRESETS, ReplacePreset, ReplacePresetRefusal, delete_in as delete_replace_preset_in,
    save_in as save_replace_preset_in, seed_in as seed_replace_presets_in,
};
use crate::tag_editor::session::{
    FileChangeView, LOCAL_VOLUME_ID, NamePreviewRow, TagEdit, TagEditorError, changes, close, edit, open_counting,
    preview_names, set_view_scope, tool as run_tool,
};
use crate::tag_editor::side_files::{SideFileChange, side_file_changes};
use crate::tag_editor::sort::{TrackSort, sort_tracks};
use crate::tag_editor::text_tools::{TagTextTool, run as run_text_tool};
use crate::tag_editor::view::TagEditorView;
use crate::tag_editor::window::TagsSessionInfo;
use crate::window_events::TagsClipboardAction;

/// How long a dropped picture's read may stand still before it's given up: a whole-file read off a
/// NAS has no honest ceiling, only a silence one (`SESSION_IO_TIMEOUT`'s scale), so it may take as
/// long as its bytes keep coming.
const COVER_READ_STALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);
/// How long converting a pasted picture (a TIFF decode and re-encode, CPU) may take.
const PASTE_CONVERT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
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

/// Starts (or, when `restart` is set, restarts) the tag session of a Multi-Rename session. The
/// read may take as long as files keep being read; once none was for `SESSION_IO_TIMEOUT` (a
/// stalled share) it's given up as `ReadTimedOut`.
#[tauri::command]
#[specta::specta]
pub async fn open_tag_editor(session_id: String, restart: bool) -> Result<TagEditorView, TagEditorError> {
    let started = Instant::now();
    let read = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&read);
    let result = blocking_typed_result_until_stalled(
        SESSION_IO_TIMEOUT,
        &mut ReadWatch::new(read),
        |_| TagEditorError::TaskFailed,
        move || open_counting(&session_id, restart, &counted),
    )
    .await;
    activity::opened(&result, started);
    result
}

/// Watches an open's reads: it stands still while no file finishes.
struct ReadWatch {
    read: Arc<AtomicUsize>,
    seen: usize,
    since: Instant,
}

impl ReadWatch {
    fn new(read: Arc<AtomicUsize>) -> Self {
        Self {
            read,
            seen: 0,
            since: Instant::now(),
        }
    }
}

impl StallWatch<TagEditorError> for ReadWatch {
    fn idle_for(&self) -> std::time::Duration {
        self.since.elapsed()
    }

    fn on_poll(&mut self) {
        let now = self.read.load(Ordering::Relaxed);
        if now != self.seen {
            self.seen = now;
            self.since = Instant::now();
        }
    }

    fn give_up(&mut self) -> Option<TagEditorError> {
        Some(TagEditorError::ReadTimedOut)
    }
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

/// Reverses the last pending edit (an edit batch, a tool run or a cover change); changes nothing
/// when there is none.
#[tauri::command]
#[specta::specta]
pub async fn undo_tag_edit(session_id: String) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = undo_edit(&session_id);
        activity::edited("undo edit", &result, started);
        result
    })
    .await
}

/// Re-applies the last undone pending edit; changes nothing when there is none.
#[tauri::command]
#[specta::specta]
pub async fn redo_tag_edit(session_id: String) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = redo_edit(&session_id);
        activity::edited("redo edit", &result, started);
        result
    })
    .await
}

/// Puts the tracks in `sort` order (the rename order moves with them; no file or edit is touched).
#[tauri::command]
#[specta::specta]
pub async fn sort_tag_tracks(session_id: String, sort: TrackSort) -> Result<TagEditorView, TagEditorError> {
    blocking(move || {
        let started = Instant::now();
        let result = sort_tracks(&session_id, sort);
        activity::edited("sort", &result, started);
        result
    })
    .await
}

/// Narrows the Album pane to the tracks at `paths` (the Tracks pane's selection), or widens it to every
/// track for `None`. Not an edit: nothing joins the undo history.
#[tauri::command]
#[specta::specta]
pub async fn set_tag_view_scope(
    session_id: String,
    paths: Option<Vec<String>>,
) -> Result<TagEditorView, TagEditorError> {
    blocking(move || set_view_scope(&session_id, paths)).await
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

/// The saved F4 Replace lists, top to bottom. Built-ins this install hasn't taken yet go in first, once.
#[tauri::command]
#[specta::specta]
pub fn get_tag_replace_presets(app: tauri::AppHandle) -> Vec<ReplacePreset> {
    REPLACE_PRESETS.edit(&app, "seeding", |list| seed_replace_presets_in(list, MAX_PRESETS))
}

/// Saves a Replace list at the end; one with the same name is replaced where it stands. Answers the
/// lists after, or why not.
#[tauri::command]
#[specta::specta]
pub fn save_tag_replace_preset(
    app: tauri::AppHandle,
    preset: ReplacePreset,
) -> Result<Vec<ReplacePreset>, ReplacePresetRefusal> {
    let mut refused = None;
    let presets = REPLACE_PRESETS.edit(&app, "a save", |list| {
        match save_replace_preset_in(&mut list.entries, preset, MAX_PRESETS) {
            Ok(()) => true,
            Err(refusal) => {
                refused = Some(refusal);
                false
            }
        }
    });
    refused.map_or(Ok(presets), Err)
}

/// Deletes a Replace list by id (a built-in stays gone). Answers the lists after.
#[tauri::command]
#[specta::specta]
pub fn delete_tag_replace_preset(app: tauri::AppHandle, id: String) -> Vec<ReplacePreset> {
    REPLACE_PRESETS.edit(&app, "a delete", |list| {
        delete_replace_preset_in(&mut list.entries, &id)
    })
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

/// What a write would do with the book's Audiobookshelf side files (`reader.txt`, `desc.txt`): the
/// frontend asks only with `multiRename.tagEditor.absSideFiles` on. None for a read-only session.
#[tauri::command]
#[specta::specta]
pub async fn get_side_file_changes(session_id: String) -> Result<Vec<SideFileChange>, TagEditorError> {
    blocking(move || side_file_changes(&session_id)).await
}

/// Writes the changed files. `continues`: this is a "Write anyway" after a press that wrote, so
/// Undo reverses both; otherwise a write that writes something is the folder's new last write.
/// No deadline on purpose: the files are local (a remote session is refused), and a write cut
/// short would leave the book half written; a quit waits for it (`tag_editor/at_quit.rs`).
#[tauri::command]
#[specta::specta]
pub async fn apply_tags(
    app: tauri::AppHandle,
    session_id: String,
    request: ApplyRequest,
    continues: bool,
) -> Result<TagApplyReport, TagEditorError> {
    blocking(move || {
        let _writing = at_quit::writing();
        let started = Instant::now();
        let result = crate::tag_editor::apply::apply(&app, &session_id, request, continues);
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
        let _writing = at_quit::writing();
        let started = Instant::now();
        let result = crate::tag_editor::undo::undo(&app, apply_id, session_id);
        activity::undone(&result, started);
        result
    })
    .await
}

/// The last tag write of the session's folder, which ⌘Z undoes: kept for the app's lifetime, so a
/// Tags window opened later over the same folder still offers it. `None` when there's none.
#[tauri::command]
#[specta::specta]
pub async fn last_tag_apply(session_id: String) -> Result<Option<LastTagApply>, TagEditorError> {
    last_of(&session_id)
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

/// Opens tracks of the session in the app `player` names (`"default"`: each in its default app;
/// else a bundle id). A chosen app that isn't installed falls back to the default and says so.
/// Returns once the launch is asked for, not when the player is up.
#[tauri::command]
#[specta::specta]
pub async fn open_tags_in_player(
    app: tauri::AppHandle,
    session_id: String,
    paths: Vec<String>,
    player: String,
) -> Result<PlayerOpened, TagEditorError> {
    blocking(move || open_in_player(&app, player_targets(&session_id, &paths)?, &player)).await
}

/// The apps that can open the session's tracks (the system default first), plus `player`'s app when
/// it is a bundle id that's installed.
#[tauri::command]
#[specta::specta]
pub async fn tag_player_choices(session_id: String, player: String) -> Result<Vec<AppChoice>, TagEditorError> {
    blocking(move || player_choices(&session_id, &player)).await
}

/// Asks the user for a player app with the system's open panel; `None` when they cancel.
#[tauri::command]
#[specta::specta]
pub async fn choose_tag_player(app: tauri::AppHandle) -> Result<Option<AppChoice>, TagEditorError> {
    choose_player(&app).await
}

/// The folder the session's tracks share, when its files are on the local disk.
fn local_folder(session_id: &str) -> Result<std::path::PathBuf, TagEditorError> {
    let files = files_of(session_id).map_err(|_| TagEditorError::NoSession)?;
    if files.volume_id == LOCAL_VOLUME_ID {
        Ok(crate::tag_editor::cover::tracks_folder(&files.dir, files.names.iter()))
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

/// Gives every file a pasted picture as its cover, in memory. The bytes come as base64 (a JSON
/// number array would make a large TIFF several times its size on both sides), refused unread
/// when longer than the largest paste taken could encode to, and decoded off the IPC thread. A
/// JPEG, PNG, or WebP image by its first bytes is taken at most 10 MiB (`CoverTooLarge`); a TIFF
/// is converted to PNG or JPEG within `PASTE_CONVERT_TIMEOUT` (`PasteTimedOut`); anything else,
/// bad base64 included, is `PastedNotAnImage`.
#[tauri::command]
#[specta::specta]
pub async fn set_cover_from_bytes(session_id: String, data: String) -> Result<TagEditorView, TagEditorError> {
    let started = Instant::now();
    let result = set_from_bytes_within(session_id, data, PASTE_CONVERT_TIMEOUT).await;
    activity::edited("cover from paste", &result, started);
    result
}

/// Gives every file a dropped picture file as its cover, in memory. Only a regular file is read
/// (a folder, FIFO, or symlink is `CoverNotAFile`), with the same type sniff and size cap. A drop
/// can come from a NAS: the read may take as long as its bytes keep coming, and is given up once
/// they stood still for `COVER_READ_STALL_LIMIT` (`CoverTimedOut`); nothing changes then or later.
#[tauri::command]
#[specta::specta]
pub async fn set_cover_from_path(session_id: String, path: String) -> Result<TagEditorView, TagEditorError> {
    let started = Instant::now();
    let result = set_from_path_within(
        session_id,
        path.into(),
        COVER_READ_STALL_LIMIT,
        Arc::clone(&DROP_READS),
        read_dropped_counted,
    )
    .await;
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
/// after opening the session; the window's close (or its `Destroyed`) releases it. It also takes
/// down any Tags window left behind with its session no longer held (`window::close_orphans`).
#[tauri::command]
#[specta::specta]
pub async fn hold_tags_session(app: tauri::AppHandle, session_id: String) -> Result<TagsSessionInfo, MultiRenameError> {
    let held = crate::tag_editor::window::hold(&session_id);
    crate::tag_editor::window::close_orphans(&app);
    held
}

/// Whether the Tags window of `session_id` may close without the discard question: no unwritten
/// edit in its session and no tag write running. Its error boundary asks before it closes the
/// window, since the edits live here and a closed window takes them along.
#[tauri::command]
#[specta::specta]
pub async fn tags_window_may_close(session_id: String) -> bool {
    crate::tag_editor::window::may_close_unasked(&session_id)
}

/// Hands Edit > Cut, Copy or Paste back to the focused Tags window as the native action
/// (`cut:`/`copy:`/`paste:` to its first responder). The menu routes the three to the window
/// (`TagsClipboard`) so they can act on a field; while a text box or the cover section has focus
/// the window sends them here, and the box (or the section's picture paste) keeps its own. Only a
/// Tags window that still has focus may: the action goes to whatever window is key.
#[tauri::command]
#[specta::specta]
pub async fn tags_native_edit(
    app: tauri::AppHandle,
    webview_window: tauri::WebviewWindow,
    action: TagsClipboardAction,
) {
    let focused = webview_window.is_focused().unwrap_or(false);
    if !crate::tag_editor::window::may_hand_back(webview_window.label(), focused) {
        log::debug!("an edit action handed back by a window without focus (or not a Tags window) was dropped");
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let id = crate::tag_editor::window::native_edit_id_of(action);
        if let Err(e) = app.run_on_main_thread(move || crate::menu::send_native_edit_action(id)) {
            log::warn!("couldn't hand an edit action back to a Tags window: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, action);
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
/// the window reads the tags itself. A selected folder opens into its audio files; that walk gets
/// [`TAGS_FOLDER_OPEN_TIMEOUT`] (a share is slow), and one that runs out is stopped and leaves no session.
#[tauri::command]
#[specta::specta]
pub async fn open_tags_session(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    expected_sequence: u64,
) -> Result<TagsOpened, MultiRenameError> {
    let timed_out = || MultiRenameError::TimedOut;
    let join_failed = |detail: String| MultiRenameError::Internal { detail };
    let plan =
        cmdr_fs::deadline::blocking_typed_result_with_timeout(TAGS_OPEN_TIMEOUT, timed_out, join_failed, move || {
            plan_for_tags(
                &listing_id,
                include_hidden,
                selected_indices.as_deref(),
                expected_sequence,
            )
        })
        .await?;
    let timeout = if plan.has_folder() {
        TAGS_FOLDER_OPEN_TIMEOUT
    } else {
        TAGS_OPEN_TIMEOUT
    };
    let handshake = Arc::new(OpenHandshake::new());
    let stop = Arc::clone(&handshake);
    cmdr_fs::deadline::blocking_typed_result_with_timeout(
        timeout,
        move || {
            stop.give_up();
            MultiRenameError::TimedOut
        },
        join_failed,
        move || plan.open(&handshake),
    )
    .await
}

const TAGS_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
/// A folder walk over an OS-mounted share reads one directory at a time.
const TAGS_FOLDER_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
