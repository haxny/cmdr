//! A tag session per Multi-Rename session: every supported audio file's tags as read from
//! disk and as edited so far, in memory, keyed by PATH.
//!
//! What this guarantees:
//! - The files are the Multi-Rename session's supported audio files at `open`. The session's
//!   names are fixed, so a reorder only changes the ORDER: the view walks the CURRENT names and
//!   looks each file up by path, and an edit never moves to another file.
//! - Each file carries a [`Fingerprint`] taken just before it was read (or after the app's own
//!   write, `mark_written`). The writer (`apply.rs`) re-checks it right before each write and
//!   refuses a file whose fingerprint moved (something else rewrote or renamed it since). Only
//!   an undo re-reads a file behind the user's edits (`reload`), dropping that file's edits.
//! - Closing or evicting the Multi-Rename session closes its tag session.
//!
//! ❌ No file read under `SESSIONS`' lock: reads happen first, the lock only swaps results in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

pub use super::error::TagEditorError;
pub use super::file_changes::changes;
pub(super) use cmdr_tags::Fingerprint;
use cmdr_tags::shared::{CoverEdit, Edit, apply_edits};
use cmdr_tags::tools::{TagTool, ToolContext, apply_tool_in};
use cmdr_tags::{
    FileTags, KindRemoval, NamePattern, TagError, TagField, compile_pattern, format_of, normalize_for, read_duration,
    read_tags,
};

use super::follow::follow_disk;
use super::history::{EditHistory, capture, clear, record};
use super::sort::{TrackSort, reorder};
use super::view::{
    Access, CoverKeys, ReadErrorCode, ReadOnlyReason, TagEditorView, build, cover_key, file_name_of, scoped_tracks,
    stem_of,
};
use crate::crash_reporter::contain_panics;
use crate::ignore_poison::IgnorePoison;
use crate::multi_rename::session::{SessionFiles, files_of};

/// The only volume v1 writes: the local disk, as Multi-Rename's apply tells it (`run.rs`).
pub(crate) const LOCAL_VOLUME_ID: &str = "root";

pub(super) const LOG_TARGET: &str = "tag_editor::session";

/// One user edit against the window's tags. Unlike the engine's `Edit`, a single track is named by its
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
    /// What the chosen kind removes beyond the fields (an MP3's conductor under Audiobook).
    pub removes: Vec<KindRemoval>,
    /// The file couldn't be read to tell what the kind removes: it may lose a field.
    pub removes_unknown: bool,
}

/// What a file-name pattern reads from one track's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NamePreviewRow {
    pub path: String,
    pub file_name: String,
    /// The fields the name gives, in pattern order; `None` when the name doesn't match.
    pub fields: Option<Vec<NameValue>>,
}

/// One field a file name gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NameValue {
    pub field: TagField,
    pub value: String,
}

/// A file read from disk.
#[derive(Debug, Clone)]
pub(super) struct Loaded {
    pub disk: FileTags,
    pub edited: FileTags,
    pub duration_ms: Option<u64>,
    /// The key of `edited`'s cover ([`cover_key`]), so a view build never hashes a cover.
    pub cover_key: Option<u64>,
    /// Taken just BEFORE the read, so a change during the read shows as a change. `None` when
    /// the file couldn't be stat'ed, which counts as changed.
    pub fingerprint: Option<Fingerprint>,
}

impl Loaded {
    pub(super) fn changed(&self) -> bool {
        self.disk != self.edited
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
pub(super) struct TagSession {
    pub(super) by_path: HashMap<PathBuf, Entry>,
    /// The order last chosen or opened in; `None` before the first sort or once another road reordered the tracks.
    pub(super) sort: Option<TrackSort>,
    /// The order `sort` made; `sort` stands only while the tracks still are in it.
    pub(super) sorted_tracks: Vec<PathBuf>,
    pub(super) ordered: bool, // A sort has run: a restart keeps the order, even one moved by hand.
    /// The pending edits' undo and redo steps.
    pub(super) history: EditHistory,
    /// The tracks the Album pane's shared rows cover (the Tracks pane's selection); `None` for every track.
    /// Not an edit: no history step.
    pub(super) view_scope: Option<HashSet<PathBuf>>,
}

pub(super) static SESSIONS: LazyLock<Mutex<HashMap<String, TagSession>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The last `TagEditorView::view_seq` handed out, taken under `SESSIONS`' lock: it follows the order sessions change in.
static VIEW_SEQ: AtomicU64 = AtomicU64::new(0);

/// The session's audio files in its current order, where they are, and what the window may do
/// with them.
pub(super) struct Layout {
    access: Access,
    volume_id: String,
    /// The folder the session's names are relative to.
    pub(super) dir: PathBuf,
    pub(super) tracks: Vec<PathBuf>,
}

pub(super) fn layout_of(files: &SessionFiles) -> Layout {
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
        volume_id: files.volume_id.clone(),
        dir: files.dir.clone(),
        tracks,
    }
}

/// Whether `std::fs` reaches the volume's files (an OS-mounted share), so they can be read.
pub(crate) fn volume_gives_local_paths(volume_id: &str) -> bool {
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
        TagError::Write(_) | TagError::BadDate(_) | TagError::WouldDrop(_) | TagError::UnwritableExtra(_) => {
            ReadErrorCode::Unreadable
        }
    }
}

/// Reads one file. Lofty may panic on a hostile file: that is a row with a code, not a crash.
fn read_entry(path: &Path) -> Entry {
    let fingerprint = Fingerprint::of(path);
    match contain_panics(|| read_tags(path)) {
        None => Entry::Unreadable(ReadErrorCode::Unreadable),
        Some(Err(error)) => Entry::Unreadable(code_of(&error)),
        Some(Ok(tags)) => {
            let duration_ms = contain_panics(|| read_duration(path))
                .flatten()
                .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
            Entry::Loaded(Box::new(Loaded {
                cover_key: tags.cover.as_ref().map(cover_key),
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
#[cfg(test)]
pub fn open(session_id: &str, restart: bool) -> Result<TagEditorView, TagEditorError> {
    open_counting(session_id, restart, &AtomicUsize::new(0))
}

/// Opens as `open` does (tests), counting each file read in `read`, so a caller can tell a slow read from a stalled one.
pub(crate) fn open_counting(
    session_id: &str,
    restart: bool,
    read: &AtomicUsize,
) -> Result<TagEditorView, TagEditorError> {
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
        .map(|path| {
            let entry = read_entry(path);
            read.fetch_add(1, Ordering::Relaxed);
            (path.clone(), entry)
        })
        .collect();
    let opened = {
        let mut sessions = SESSIONS.lock_ignore_poison();
        let session = sessions.entry(session_id.to_string()).or_default();
        session.by_path = by_path;
        clear(session);
        session.drop_stale_sort(&layout);
        let ordered = session.ordered.then(|| view_of(&layout, session));
        ordered.ok_or_else(|| TrackSort::default_for(session))
    };
    let view = opened.or_else(|default| reorder(session_id, default))?;
    // The rename session may have closed during the read, after its close hook already ran:
    // it removes its own session before calling the hook, so asking again now (lock order
    // rename → tag, never nested) catches every such close.
    if files_of(session_id).is_err() {
        close(session_id);
        return Err(TagEditorError::NoSession);
    }
    Ok(view)
}

/// Runs `f` on the open session's readable files in current order (their edited tags and cover keys).
pub(super) fn with_loaded<R>(session_id: &str, f: impl FnOnce(&[&Loaded]) -> R) -> Result<R, TagEditorError> {
    with_session(session_id, |layout, session| {
        let loaded: Vec<&Loaded> = layout
            .tracks
            .iter()
            .filter_map(|path| session.by_path.get(path)?.loaded())
            .collect();
        Ok(f(&loaded))
    })
}

/// Drops entries whose path left the layout, and the sort when another road reordered the tracks.
pub(super) fn prune(session: &mut TagSession, layout: &Layout) {
    let current: HashSet<&PathBuf> = layout.tracks.iter().collect();
    session.by_path.retain(|path, _| current.contains(path));
    session.drop_stale_sort(layout);
}

pub(super) fn view_of(layout: &Layout, session: &TagSession) -> TagEditorView {
    let tracks: Vec<(&Path, &Entry)> = layout
        .tracks
        .iter()
        .filter_map(|path| session.by_path.get(path).map(|entry| (path.as_path(), entry)))
        .collect();
    let scoped = scoped_tracks(&tracks, session.view_scope.as_ref());
    let mut view = build(layout.access, &tracks, scoped.as_deref());
    // The File column drops the folder every track shares; the session's own names stay relative to its folder.
    let names: Vec<String> = tracks.iter().map(|(path, _)| file_name_of(&layout.dir, path)).collect();
    let shared = super::cover::tracks_folder(&layout.dir, names.iter());
    for (row, (path, _)) in view.tracks.iter_mut().zip(&tracks) {
        row.file_name = path.strip_prefix(&shared).map_or_else(
            |_| file_name_of(&layout.dir, path),
            |name| name.to_string_lossy().into_owned(),
        );
    }
    TagEditorView {
        sort: session.sort,
        can_undo_edit: session.history.can_undo(),
        can_redo_edit: session.history.can_redo(),
        view_seq: VIEW_SEQ.fetch_add(1, Ordering::Relaxed) + 1,
        ..view
    }
}

/// Narrows the shared rows to the tracks at `paths` (`TrackRow::path`); `None`, or none the session has, is
/// every track. Not an edit: the history and the edits stay as they are.
pub fn set_view_scope(session_id: &str, paths: Option<Vec<String>>) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        let known = |paths: Vec<String>| {
            paths
                .into_iter()
                .map(PathBuf::from)
                .filter(|path| session.by_path.contains_key(path))
        };
        session.view_scope = paths
            .map(|paths| known(paths).collect::<HashSet<_>>())
            .filter(|scope| !scope.is_empty());
        Ok(view_of(layout, session))
    })
}

/// Runs `f` on the open session, after dropping entries that left the layout.
pub(super) fn with_session<R>(
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

/// The readable tracks at `scope` (paths as `TrackRow::path` gives them) in current order, or every
/// readable track for `None`. A path the session doesn't have is `UnknownTrack`; an unreadable track
/// in the scope is left out (it is never written).
fn scoped(
    layout: &Layout,
    session: &TagSession,
    scope: Option<&[String]>,
) -> Result<(Vec<PathBuf>, Vec<FileTags>), TagEditorError> {
    let (paths, tags) = readable(layout, session);
    let Some(scope) = scope else {
        return Ok((paths, tags));
    };
    let wanted: HashSet<&Path> = scope.iter().map(Path::new).collect();
    if wanted.iter().any(|path| !session.by_path.contains_key(*path)) {
        return Err(TagEditorError::UnknownTrack);
    }
    Ok(paths
        .into_iter()
        .zip(tags)
        .filter(|(path, _)| wanted.contains(path.as_path()))
        .unzip())
}

/// The folder a file is in, as text.
fn folder_text(path: &Path) -> String {
    path.parent()
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What the tools know about `paths`, a selection of a book of `session_files` listed tracks (readable or not):
/// each one's folder and stem, and the book's size (for the one-file-book rule).
fn context_of(paths: &[PathBuf], session_files: usize, book_folders: Vec<String>) -> ToolContext {
    ToolContext {
        session_files,
        book_folders,
        folders: paths.iter().map(|path| folder_text(path)).collect(),
        stems: paths.iter().map(|path| stem_of(path)).collect(),
    }
}

/// `tags` as the file at `path` will hold them (an M4A track `01` is `1`); as given when the
/// name has no supported format.
fn as_format_holds(path: &Path, tags: FileTags) -> FileTags {
    let held = match path.file_name().and_then(|name| format_of(&name.to_string_lossy())) {
        Some(format) => normalize_for(format, &tags),
        None => tags,
    };
    drop_orphan_totals(held)
}

/// `tags` without a TrackTotal or DiscTotal whose Track or Disc number is empty.
fn drop_orphan_totals(mut tags: FileTags) -> FileTags {
    for (total, number) in [
        (TagField::TrackTotal, TagField::Track),
        (TagField::DiscTotal, TagField::Disc),
    ] {
        let numbered = tags
            .fields
            .get(&number)
            .is_some_and(|values| values.iter().any(|value| !value.trim().is_empty()));
        if !numbered {
            tags.fields.remove(&total);
        }
    }
    tags
}

/// Stores `edited` (parallel to `paths`) as the files' edited tags, as each file's format holds
/// them, so every edit path (edits, tools) shows what a write will leave. A total whose number is
/// absent is dropped the same way: it wouldn't be written.
/// Each cover keeps its key ([`CoverKeys`]).
fn store(session: &mut TagSession, paths: Vec<PathBuf>, edited: Vec<FileTags>) {
    let mut keys = CoverKeys::default();
    for (path, tags) in paths.into_iter().zip(edited) {
        if let Some(Entry::Loaded(loaded)) = session.by_path.get_mut(&path) {
            keys.remember(&loaded.edited.cover, loaded.cover_key);
            loaded.edited = as_format_holds(&path, tags);
            loaded.cover_key = keys.key_of(&loaded.edited.cover);
        }
    }
}

pub(super) fn require_editable(layout: &Layout) -> Result<(), TagEditorError> {
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
        let snapshot = capture(session);
        store(session, paths, after);
        record(session, snapshot);
        Ok(view_of(layout, session))
    })
}

/// Runs a tool over the readable tracks at `scope` (every readable track for `None`), in session
/// order; the others keep their tags. An unknown path refuses the run (`UnknownTrack`), a bad
/// file-name pattern refuses it before anything is touched (`BadPattern`), and an empty scope
/// changes nothing.
pub fn tool(session_id: &str, tool: TagTool, scope: Option<Vec<String>>) -> Result<TagEditorView, TagEditorError> {
    // Checked here because the engine leaves the files unchanged on a bad pattern: this is the
    // only place it becomes a typed answer.
    if let TagTool::FromFileName { pattern } = &tool {
        compile_pattern(pattern).map_err(|error| TagEditorError::BadPattern { error })?;
    }
    // The folders of the whole book, so a selection numbers folders as the whole run would.
    let book_folders = with_session(session_id, |layout, _| {
        Ok(layout.tracks.iter().map(|path| folder_text(path)).collect())
    })?;
    edit_scoped(session_id, scope, |paths, before, book| {
        Ok(apply_tool_in(before, &context_of(paths, book, book_folders), &tool))
    })
}

/// Replaces the tags of the readable tracks at `scope` (every readable track for `None`, in session
/// order) with what `f` returns for them, parallel to its input. `f` gets the scoped paths, their
/// edited tags, and the book's size (every audio track the session lists, readable or not). The others keep
/// their tags; an unknown path refuses the run (`UnknownTrack`), an empty scope changes nothing
/// (`f` isn't called), and an error from `f` leaves everything as it was.
pub(super) fn edit_scoped(
    session_id: &str,
    scope: Option<Vec<String>>,
    f: impl FnOnce(&[PathBuf], &[FileTags], usize) -> Result<Vec<FileTags>, TagEditorError>,
) -> Result<TagEditorView, TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        let (paths, before) = scoped(layout, session, scope.as_deref())?;
        if !paths.is_empty() {
            // Every audio track the session lists: a damaged file is still part of the book.
            let book = layout.tracks.len();
            let after = f(&paths, &before, book)?;
            let snapshot = capture(session);
            store(session, paths, after);
            record(session, snapshot);
        }
        Ok(view_of(layout, session))
    })
}

/// What `pattern` reads from the name of each readable track at `scope` (every one for `None`), in
/// session order. Changes nothing, so a read-only session previews too. The names are matched
/// after the lock.
pub fn preview_names(
    session_id: &str,
    pattern: NamePattern,
    scope: Option<Vec<String>>,
) -> Result<Vec<NamePreviewRow>, TagEditorError> {
    let compiled = compile_pattern(&pattern).map_err(|error| TagEditorError::BadPattern { error })?;
    let (paths, dir) = with_session(session_id, |layout, session| {
        Ok((scoped(layout, session, scope.as_deref())?.0, layout.dir.clone()))
    })?;
    Ok(paths
        .iter()
        .map(|path| NamePreviewRow {
            path: path.to_string_lossy().into_owned(),
            file_name: file_name_of(&dir, path),
            fields: compiled.read(&stem_of(path)).map(|pairs| {
                pairs
                    .into_iter()
                    .map(|(field, value)| NameValue { field, value })
                    .collect()
            }),
        })
        .collect())
}

/// Whether the file at `path` is still the one the session read, so a write may go ahead. A
/// path that isn't a readable track of the session is not: refuse it.
#[cfg(test)]
pub(crate) fn unchanged_on_disk(session_id: &str, path: &Path) -> Result<bool, TagEditorError> {
    // The fingerprint is copied out under the lock; the stat runs after it.
    let recorded = with_session(session_id, |_, session| {
        Ok(session
            .by_path
            .get(path)
            .and_then(Entry::loaded)
            .map(|loaded| loaded.fingerprint.clone()))
    })?;
    Ok(recorded.is_some_and(|fingerprint| Fingerprint::holds(fingerprint.as_ref(), path)))
}

/// A file the write would change: its path, the tags to write, and the fingerprint of the
/// copy the session read (or last wrote).
pub(super) struct Pending {
    pub path: PathBuf,
    pub tags: FileTags,
    pub fingerprint: Option<Fingerprint>,
}

/// Where a session's files are, for the writer's media-cache refresh.
pub(super) struct Target {
    pub volume_id: String,
}

/// The readable files whose edited tags differ from the disk's, in current order, minus those
/// `skip` says. Refuses a read-only session. Copies only: no file is touched.
pub(super) fn pending(
    session_id: &str,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<(Target, Vec<Pending>), TagEditorError> {
    with_session(session_id, |layout, session| {
        require_editable(layout)?;
        let pending = layout
            .tracks
            .iter()
            .filter(|path| !skip(path))
            .filter_map(|path| {
                let loaded = session.by_path.get(path)?.loaded()?;
                loaded.changed().then(|| Pending {
                    path: path.clone(),
                    tags: loaded.edited.clone(),
                    fingerprint: loaded.fingerprint.clone(),
                })
            })
            .collect();
        let target = Target {
            volume_id: layout.volume_id.clone(),
        };
        Ok((target, pending))
    })
}

/// After the app's own write: each file is read again (before the lock), and the session's
/// disk copy and fingerprint become what is really on disk now, so the next apply doesn't take
/// the write for someone else's. The edited copy follows when it is still what was written (a
/// "Write anyway" may have dropped items); an edit made during the write is kept and still
/// shows as a change. A file that no longer reads becomes an unreadable row. A no-op for a
/// session or file that's gone.
pub(super) fn mark_written(session_id: &str, written: Vec<(PathBuf, FileTags)>) {
    if written.is_empty() {
        // Nothing moved on disk: every edit is still pending, and so is its undo.
        return;
    }
    let fresh: Vec<(PathBuf, FileTags, Entry)> = written
        .into_iter()
        .map(|(path, requested)| {
            let entry = read_entry(&path);
            (path, requested, entry)
        })
        .collect();
    let mut sessions = SESSIONS.lock_ignore_poison();
    let Some(session) = sessions.get_mut(session_id) else {
        return;
    };
    clear(session);
    for (path, requested, entry) in fresh {
        let Some(slot) = session.by_path.get_mut(&path) else {
            continue;
        };
        match (slot, entry) {
            (Entry::Loaded(loaded), Entry::Loaded(now)) => {
                if loaded.edited == requested {
                    loaded.edited = now.disk.clone();
                    loaded.cover_key = now.cover_key;
                }
                loaded.disk = now.disk;
                loaded.fingerprint = now.fingerprint;
                loaded.duration_ms = now.duration_ms;
            }
            (slot, entry) => {
                if let Entry::Unreadable(code) = &entry {
                    // The row's unwritten edits go with it; the code is typed, and no path is logged.
                    log::warn!(target: LOG_TARGET, "A file couldn't be read after its write, so its row is unreadable now: {code:?}");
                }
                *slot = entry;
            }
        }
    }
}

/// Reads `paths` again (after an undo rewrote them) into every open tag session that holds them:
/// the undo may come from another sheet than the one that wrote. The `undoing` session (whose
/// user confirmed losing unwritten edits to those files) takes them as read. Every other session
/// keeps its unwritten edits: only the parts it never edited follow the disk ([`super::follow::rebase`]), and
/// its disk copy and fingerprint become what is on disk now. Each file is read once, outside the
/// lock; only the tag lock is taken, so the rename → tag lock order holds.
pub(super) fn reload(paths: &[PathBuf], undoing: Option<&str>) {
    let held: Vec<PathBuf> = {
        let sessions = SESSIONS.lock_ignore_poison();
        paths
            .iter()
            .filter(|path| sessions.values().any(|session| session.by_path.contains_key(*path)))
            .cloned()
            .collect()
    };
    let fresh: Vec<(PathBuf, Entry)> = held
        .into_iter()
        .map(|path| {
            let entry = read_entry(&path);
            (path, entry)
        })
        .collect();
    let mut sessions = SESSIONS.lock_ignore_poison();
    for (session_id, session) in sessions.iter_mut() {
        let take_as_read = undoing == Some(session_id.as_str());
        if fresh.iter().any(|(path, _)| session.by_path.contains_key(path)) {
            // The files moved under the steps; they'd put back stale tags.
            clear(session);
        }
        for (path, entry) in &fresh {
            if let Some(slot) = session.by_path.get_mut(path) {
                if take_as_read {
                    *slot = entry.clone();
                } else {
                    follow_disk(slot, entry);
                }
            }
        }
    }
}

/// Ends the tag session, dropping its edits. A no-op when it's already gone.
pub fn close(session_id: &str) {
    SESSIONS.lock_ignore_poison().remove(session_id);
    // After the store's lock is gone: the two locks never nest.
    super::playback::release_session(session_id);
}

/// The readable tracks of the open session, as the session names them, in current order.
pub(super) fn loaded_tracks(session_id: &str) -> Result<Vec<PathBuf>, TagEditorError> {
    with_session(session_id, |layout, session| {
        Ok(layout
            .tracks
            .iter()
            .filter(|path| session.by_path.get(*path).is_some_and(|entry| entry.loaded().is_some()))
            .cloned()
            .collect())
    })
}

/// How many tag sessions hold edits not yet written: a quit asks about them (`at_quit.rs`).
pub(crate) fn unwritten_sessions() -> usize {
    let sessions = SESSIONS.lock_ignore_poison();
    sessions.values().filter(|s| holds_unwritten(s)).count()
}

/// Whether `session_id`'s tag session holds unwritten edits (`false` if gone): `window::may_close_unasked`.
pub(crate) fn has_unwritten(session_id: &str) -> bool {
    SESSIONS
        .lock_ignore_poison()
        .get(session_id)
        .is_some_and(holds_unwritten)
}

fn holds_unwritten(session: &TagSession) -> bool {
    session
        .by_path
        .values()
        .any(|entry| entry.loaded().is_some_and(Loaded::changed))
}

/// Whether a tag session exists for `session_id`.
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
