//! A tag session per Multi-Rename session: every supported audio file's tags as read from
//! disk and as edited so far, in memory, keyed by PATH.
//!
//! The view walks the Multi-Rename session's CURRENT names and looks each file up by path, so
//! a reorder never moves an edit to another file. A path the session doesn't know yet (a file
//! a Multi-Rename apply renamed while the tab was open) is read from disk then; edits made
//! under its old name stay with that name and are never applied to another file.
//!
//! ❌ No file read under `SESSIONS`' lock: reads happen first, the lock only swaps results in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use cmdr_tags::shared::{Edit, apply_edits, changes as engine_changes};
use cmdr_tags::tools::{TagTool, apply_tool};
use cmdr_tags::{FileTags, TagError, TagField, format_of, read_duration, read_tags};

use super::view::{Access, ReadErrorCode, ReadOnlyReason, TagEditorView, build};
use crate::ignore_poison::IgnorePoison;
use crate::multi_rename::session::{SessionFiles, files_of};

/// The only volume v1 writes: the local disk, as Multi-Rename's apply tells it (`run.rs`).
const LOCAL_VOLUME_ID: &str = "root";

/// Why a tag-editor call didn't answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TagEditorError {
    /// The Multi-Rename session is gone.
    NoSession,
    /// `open` wasn't called for this session.
    NotOpen,
    /// The session's volume is read-only for the tag editor.
    ReadOnly,
    /// A file couldn't be read.
    Read { path: String, detail: String },
}

/// What a file's change looks like when the user reviews it before writing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeView {
    pub path: String,
    pub file_name: String,
    pub fields: Vec<TagField>,
    pub cover: bool,
    pub extras: bool,
}

/// A file read from disk.
#[derive(Debug, Clone)]
pub(super) struct Loaded {
    pub disk: FileTags,
    pub edited: FileTags,
    pub duration_ms: Option<u64>,
}

impl Loaded {
    pub(super) fn changed(&self) -> bool {
        self.disk != self.edited
    }
}

/// One audio file: readable, or listed and never written.
#[derive(Debug, Clone)]
pub(super) enum Entry {
    Loaded(Loaded),
    Unreadable(ReadErrorCode),
}

impl Entry {
    pub(super) fn loaded(&self) -> Option<&Loaded> {
        match self {
            Entry::Loaded(loaded) => Some(loaded),
            Entry::Unreadable(_) => None,
        }
    }
}

#[derive(Default)]
struct TagSession {
    by_path: HashMap<PathBuf, Entry>,
}

static SESSIONS: LazyLock<Mutex<HashMap<String, TagSession>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The session's audio files in its current order, and what the tab may do with them.
struct Layout {
    access: Access,
    tracks: Vec<PathBuf>,
}

fn layout_of(files: &SessionFiles) -> Layout {
    let local = files.volume_id == LOCAL_VOLUME_ID;
    let reachable = local || volume_gives_local_paths(&files.volume_id);
    let tracks = if reachable {
        files
            .names
            .iter()
            .filter(|name| format_of(name).is_some())
            .map(|name| files.dir.join(name))
            .collect()
    } else {
        Vec::new()
    };
    Layout {
        access: Access {
            editable: local,
            reason: (!local).then_some(ReadOnlyReason::RemoteVolume),
        },
        tracks,
    }
}

/// Whether `std::fs` reaches the volume's files (an OS-mounted share), so they can be read.
fn volume_gives_local_paths(volume_id: &str) -> bool {
    crate::file_system::volume::manager::get_volume_manager()
        .get(volume_id)
        .is_some_and(|volume| volume.supports_local_fs_access())
}

fn layout(session_id: &str) -> Result<Layout, TagEditorError> {
    let files = files_of(session_id).map_err(|_| TagEditorError::NoSession)?;
    Ok(layout_of(&files))
}

fn code_of(error: &TagError) -> ReadErrorCode {
    match error {
        TagError::NotSupported => ReadErrorCode::Unsupported,
        TagError::Io(..) => ReadErrorCode::Io,
        TagError::Parse(_) => ReadErrorCode::Parse,
        TagError::Write(_) | TagError::WouldDrop(_) | TagError::UnwritableExtra(_) => ReadErrorCode::Unreadable,
    }
}

/// Reads one file. Lofty may panic on a hostile file: that is a row with a code, not a crash.
fn read_entry(path: &Path) -> Entry {
    use crate::crash_reporter::contain_panics;
    match contain_panics(|| read_tags(path)) {
        None => Entry::Unreadable(ReadErrorCode::Unreadable),
        Some(Err(error)) => Entry::Unreadable(code_of(&error)),
        Some(Ok(tags)) => {
            let duration_ms = contain_panics(|| read_duration(path))
                .flatten()
                .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
            Entry::Loaded(Loaded {
                disk: tags.clone(),
                edited: tags,
                duration_ms,
            })
        }
    }
}

fn read_all(paths: &[PathBuf]) -> Vec<(PathBuf, Entry)> {
    paths.iter().map(|path| (path.clone(), read_entry(path))).collect()
}

/// Starts (or restarts) the tag session of Multi-Rename session `session_id`: reads every
/// supported file. A file that can't be read becomes a row with a `readError` and is never
/// written. Blocking file work: call it off the IPC thread.
pub fn open(session_id: &str) -> Result<TagEditorView, TagEditorError> {
    let layout = layout(session_id)?;
    let by_path: HashMap<PathBuf, Entry> = read_all(&layout.tracks).into_iter().collect();
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.entry(session_id.to_string()).or_default();
    session.by_path = by_path;
    Ok(view_of(&layout, session))
}

fn view_of(layout: &Layout, session: &TagSession) -> TagEditorView {
    let tracks: Vec<(&Path, &Entry)> = layout
        .tracks
        .iter()
        .filter_map(|path| session.by_path.get(path).map(|entry| (path.as_path(), entry)))
        .collect();
    build(layout.access, &tracks)
}

/// Runs `f` on the session with every current track loaded: tracks the session doesn't know
/// yet are read first, outside the lock.
fn with_session<R>(
    session_id: &str,
    f: impl FnOnce(&Layout, &mut TagSession) -> Result<R, TagEditorError>,
) -> Result<R, TagEditorError> {
    let layout = layout(session_id)?;
    let missing: Vec<PathBuf> = {
        let sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.get(session_id).ok_or(TagEditorError::NotOpen)?;
        layout
            .tracks
            .iter()
            .filter(|path| !session.by_path.contains_key(*path))
            .cloned()
            .collect()
    };
    let fresh = read_all(&missing);
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get_mut(session_id).ok_or(TagEditorError::NotOpen)?;
    for (path, entry) in fresh {
        session.by_path.entry(path).or_insert(entry);
    }
    f(&layout, session)
}

/// The readable tracks in current order: their paths and edited tags.
fn readable(layout: &Layout, session: &TagSession) -> (Vec<PathBuf>, Vec<FileTags>) {
    layout
        .tracks
        .iter()
        .filter_map(|path| {
            let loaded = session.by_path.get(path)?.loaded()?;
            Some((path.clone(), loaded.edited.clone()))
        })
        .unzip()
}

/// Stores `edited` (parallel to `paths`) as the files' edited tags.
fn store(session: &mut TagSession, paths: Vec<PathBuf>, edited: Vec<FileTags>) {
    for (path, tags) in paths.into_iter().zip(edited) {
        if let Some(Entry::Loaded(loaded)) = session.by_path.get_mut(&path) {
            loaded.edited = tags;
        }
    }
}

fn require_editable(layout: &Layout) -> Result<(), TagEditorError> {
    if layout.access.editable {
        Ok(())
    } else {
        Err(TagEditorError::ReadOnly)
    }
}

/// `SetOne` names a row of the track list as shown, unreadable rows included; the engine
/// wants the index among the readable files. An edit on an unreadable row is dropped.
fn translate(layout: &Layout, session: &TagSession, edits: Vec<Edit>) -> Vec<Edit> {
    let mut next = 0;
    let readable_index: Vec<Option<usize>> = layout
        .tracks
        .iter()
        .map(|path| {
            session.by_path.get(path)?.loaded()?;
            next += 1;
            Some(next - 1)
        })
        .collect();
    edits
        .into_iter()
        .filter_map(|edit| match edit {
            Edit::SetOne { row, field, values } => {
                readable_index
                    .get(row)
                    .copied()
                    .flatten()
                    .map(|row| Edit::SetOne { row, field, values })
            }
            other => Some(other),
        })
        .collect()
}

/// Applies `edits` in memory. `SetOne` rows are rows of `tracks` as the last view showed them.
pub fn edit(session_id: &str, edits: Vec<Edit>) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        let (paths, before) = readable(layout, session);
        let after = apply_edits(&before, &translate(layout, session, edits));
        store(session, paths, after);
        Ok(view_of(layout, session))
    })
}

/// Runs a clean-up tool over the WHOLE session's readable files (never a sub-selection: the
/// renumber and single-file rules depend on seeing all of them).
pub fn tool(session_id: &str, tool: TagTool) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        let (paths, before) = readable(layout, session);
        let after = apply_tool(&before, &tool);
        store(session, paths, after);
        Ok(view_of(layout, session))
    })
}

/// What a write would change, per file, in current order.
pub fn changes(session_id: &str) -> Result<Vec<FileChangeView>, TagEditorError> {
    with_session(session_id, |layout, session| {
        let (paths, _) = readable(layout, session);
        let (disk, edited): (Vec<FileTags>, Vec<FileTags>) = layout
            .tracks
            .iter()
            .filter_map(|path| session.by_path.get(path)?.loaded())
            .map(|loaded| (loaded.disk.clone(), loaded.edited.clone()))
            .unzip();
        Ok(engine_changes(&disk, &edited)
            .into_iter()
            .filter_map(|change| {
                let path = paths.get(change.row)?;
                Some(FileChangeView {
                    path: path.to_string_lossy().into_owned(),
                    file_name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    fields: change.fields,
                    cover: change.cover,
                    extras: change.extras,
                })
            })
            .collect())
    })
}

/// Ends the tag session, dropping its edits. A no-op when it's already gone.
pub fn close(session_id: &str) {
    SESSIONS.lock_ignore_poison().remove(session_id);
}
