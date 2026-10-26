//! A tag session per Multi-Rename session: every supported audio file's tags as read from
//! disk and as edited so far, in memory, keyed by PATH.
//!
//! What this guarantees:
//! - The files are the Multi-Rename session's supported audio files at `open`. The session's
//!   names are fixed, so a reorder only changes the ORDER: the view walks the CURRENT names and
//!   looks each file up by path, and an edit never moves to another file.
//! - Each file carries a [`Fingerprint`] taken just before it was read. A write must call
//!   [`unchanged_on_disk`] first and refuse a file whose fingerprint moved (something else
//!   rewrote or renamed it since). Nothing re-reads a file behind the user's edits.
//! - Closing or evicting the Multi-Rename session closes its tag session.
//!
//! ❌ No file read under `SESSIONS`' lock: reads happen first, the lock only swaps results in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use cmdr_tags::shared::{CoverEdit, Edit, apply_edits, changes as engine_changes};
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
    /// An edit names a path that isn't a readable track of the session. Nothing was applied.
    UnknownTrack,
    /// A file couldn't be read.
    Read { path: String, detail: String },
}

/// One user edit against the tab. Unlike the engine's `Edit`, a single track is named by its
/// PATH (as `TrackRow::path` shows it), so a reorder between the view and the edit can't send
/// the edit to another file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TagEdit {
    /// Set the field on every file. Empty values remove it.
    SetAll { field: TagField, values: Vec<String> },
    /// Set the field on the track at `path`.
    SetOne {
        path: String,
        field: TagField,
        values: Vec<String>,
    },
    /// Change the cover of every file.
    Cover { edit: CoverEdit },
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

/// What identifies a file's state on disk: size, modification time, and (on unix) inode.
/// A rewrite, a replace, and a rename-over each move at least one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Fingerprint {
    size: u64,
    modified: Option<SystemTime>,
    inode: Option<u64>,
}

impl Fingerprint {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            size: meta.len(),
            modified: meta.modified().ok(),
            inode: inode_of(&meta),
        })
    }
}

#[cfg(unix)]
fn inode_of(meta: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino())
}

#[cfg(not(unix))]
fn inode_of(_meta: &std::fs::Metadata) -> Option<u64> {
    None
}

/// A file read from disk.
#[derive(Debug, Clone)]
pub(super) struct Loaded {
    pub disk: FileTags,
    pub edited: FileTags,
    pub duration_ms: Option<u64>,
    /// Taken just BEFORE the read, so a change during the read shows as a change. `None` when
    /// the file couldn't be stat'ed, which counts as changed.
    pub fingerprint: Option<Fingerprint>,
}

impl Loaded {
    pub(super) fn changed(&self) -> bool {
        self.disk != self.edited
    }

    /// Whether the file at `path` is still the one that was read.
    pub(super) fn unchanged_on_disk(&self, path: &Path) -> bool {
        self.fingerprint.is_some() && self.fingerprint == Fingerprint::of(path)
    }
}

/// One audio file: readable, or listed and never written.
#[derive(Debug, Clone)]
pub(super) enum Entry {
    Loaded(Box<Loaded>),
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

/// The layout, or `NoSession`, which also drops the tag session left behind.
fn layout(session_id: &str) -> Result<Layout, TagEditorError> {
    match files_of(session_id) {
        Ok(files) => Ok(layout_of(&files)),
        Err(_) => {
            close(session_id);
            Err(TagEditorError::NoSession)
        }
    }
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
    let fingerprint = Fingerprint::of(path);
    match contain_panics(|| read_tags(path)) {
        None => Entry::Unreadable(ReadErrorCode::Unreadable),
        Some(Err(error)) => Entry::Unreadable(code_of(&error)),
        Some(Ok(tags)) => {
            let duration_ms = contain_panics(|| read_duration(path))
                .flatten()
                .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
            Entry::Loaded(Box::new(Loaded {
                disk: tags.clone(),
                edited: tags,
                duration_ms,
                fingerprint,
            }))
        }
    }
}

/// Starts the tag session of Multi-Rename session `session_id`, reading every supported file.
/// A file that can't be read becomes a row with a `readError` and is never written.
///
/// With `restart: false` an existing tag session is returned as it is, edits kept; with
/// `restart: true` (or none yet) every file is read again and the edits are dropped.
/// Blocking file work: call it off the IPC thread.
pub fn open(session_id: &str, restart: bool) -> Result<TagEditorView, TagEditorError> {
    let layout = layout(session_id)?;
    if !restart {
        let mut sessions = SESSIONS.lock_ignore_poison();
        if let Some(session) = sessions.get_mut(session_id) {
            prune(session, &layout);
            return Ok(view_of(&layout, session));
        }
    }
    let by_path: HashMap<PathBuf, Entry> = layout
        .tracks
        .iter()
        .map(|path| (path.clone(), read_entry(path)))
        .collect();
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.entry(session_id.to_string()).or_default();
    session.by_path = by_path;
    Ok(view_of(&layout, session))
}

/// Drops entries whose path isn't in the layout any more.
fn prune(session: &mut TagSession, layout: &Layout) {
    let current: HashSet<&PathBuf> = layout.tracks.iter().collect();
    session.by_path.retain(|path, _| current.contains(path));
}

fn view_of(layout: &Layout, session: &TagSession) -> TagEditorView {
    let tracks: Vec<(&Path, &Entry)> = layout
        .tracks
        .iter()
        .filter_map(|path| session.by_path.get(path).map(|entry| (path.as_path(), entry)))
        .collect();
    build(layout.access, &tracks)
}

/// Runs `f` on the open session, after dropping entries that left the layout.
fn with_session<R>(
    session_id: &str,
    f: impl FnOnce(&Layout, &mut TagSession) -> Result<R, TagEditorError>,
) -> Result<R, TagEditorError> {
    let layout = layout(session_id)?;
    let mut sessions = SESSIONS.lock_ignore_poison();
    let session = sessions.get_mut(session_id).ok_or(TagEditorError::NotOpen)?;
    prune(session, &layout);
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

/// The engine's edits for `edits`: a path becomes the track's index among the READABLE files
/// (the list the engine edits). A path that isn't a readable track of the session refuses the
/// whole batch, so a stale or mistyped path never half-applies.
fn translate(paths: &[PathBuf], edits: Vec<TagEdit>) -> Result<Vec<Edit>, TagEditorError> {
    edits
        .into_iter()
        .map(|edit| match edit {
            TagEdit::SetAll { field, values } => Ok(Edit::SetAll { field, values }),
            TagEdit::Cover { edit } => Ok(Edit::Cover { edit }),
            TagEdit::SetOne { path, field, values } => paths
                .iter()
                .position(|candidate| candidate.as_os_str() == Path::new(&path).as_os_str())
                .map(|row| Edit::SetOne { row, field, values })
                .ok_or(TagEditorError::UnknownTrack),
        })
        .collect()
}

/// Applies `edits` in memory.
pub fn edit(session_id: &str, edits: Vec<TagEdit>) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        let (paths, before) = readable(layout, session);
        let engine_edits = translate(&paths, edits)?;
        let after = apply_edits(&before, &engine_edits);
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
        Ok(layout
            .tracks
            .iter()
            .filter_map(|path| {
                let loaded = session.by_path.get(path)?.loaded()?;
                let change = engine_changes(std::slice::from_ref(&loaded.disk), std::slice::from_ref(&loaded.edited))
                    .into_iter()
                    .next()?;
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

/// Whether the file at `path` is still the one the session read, so a write may go ahead. A
/// path that isn't a readable track of the session is not: refuse it.
pub(crate) fn unchanged_on_disk(session_id: &str, path: &Path) -> Result<bool, TagEditorError> {
    with_session(session_id, |_, session| {
        Ok(session
            .by_path
            .get(path)
            .and_then(Entry::loaded)
            .is_some_and(|loaded| loaded.unchanged_on_disk(path)))
    })
}

/// Ends the tag session, dropping its edits. A no-op when it's already gone.
pub fn close(session_id: &str) {
    SESSIONS.lock_ignore_poison().remove(session_id);
}

/// Whether a tag session exists for `session_id`.
#[cfg(test)]
pub(crate) fn is_open(session_id: &str) -> bool {
    SESSIONS.lock_ignore_poison().contains_key(session_id)
}

/// Puts back an empty tag session for `session_id`, as one left behind by a rename session that
/// went without telling us.
#[cfg(test)]
pub(crate) fn reinsert_for_test(session_id: &str) {
    SESSIONS
        .lock_ignore_poison()
        .insert(session_id.to_string(), TagSession::default());
}
