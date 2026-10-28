//! What the Tags window shows: the foobar2000-style shared rows, the per-track list in the
//! Multi-Rename session's CURRENT order, and the cover, extras, and diacritics summaries.
//! Pure: built from the session's entries, never reads a file.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use cmdr_tags::shared::{SharedValue, ValueCount, distinct_values, shared_view};
use cmdr_tags::tools::fields_with_diacritics;
use cmdr_tags::{Cover, FileTags, TagField, date_is_writable, format_of};

use super::session::{Entry, Loaded};
use super::sort::TrackSort;

/// How many distinct values a «multiple values» row lists.
const CHOICE_LIMIT: usize = 50;

/// Why the window can't write. A stable code, worded by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReadOnlyReason {
    /// The session's folder isn't on the local disk; v1 writes local files only.
    RemoteVolume,
}

/// Why a file has no tags in the list. A stable code, worded by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReadErrorCode {
    /// The tag parser gave up on the file, or panicked on it.
    Unreadable,
    /// The file's format isn't one the editor handles.
    Unsupported,
    /// The file couldn't be opened or read.
    Io,
    /// The file's tags couldn't be parsed.
    Parse,
}

/// One shared row: a field and what the files say about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedRow {
    pub field: TagField,
    pub value: SharedValue,
    /// For a `Multiple` row, the files' distinct values, most common first (at most 50); empty otherwise.
    pub choices: Vec<ValueCount>,
    /// How many more distinct values the files hold than `choices` lists (the 50 cut-off); 0 when all are listed.
    pub choices_more: u32,
}

/// One audio file in the per-track list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackRow {
    pub path: String,
    pub file_name: String,
    pub track: Option<String>,
    pub title: Option<String>,
    pub duration_ms: Option<u64>,
    /// Edited tags differ from the ones on disk.
    pub changed: bool,
    /// Set when the file couldn't be read: the row is shown greyed and never written.
    pub read_error: Option<ReadErrorCode>,
}

/// The front cover across the files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoverSummary {
    /// No file has a cover.
    Empty,
    /// The files have different covers, or only some have one.
    Multiple,
    /// Every file has this cover.
    #[serde(rename_all = "camelCase")]
    Same {
        mime: String,
        bytes: usize,
        /// Pixel size, when the image header could be read.
        width: Option<u32>,
        height: Option<u32>,
        /// A hash of the bytes, so the thumbnail follows a new cover of the same type and size.
        key: String,
    },
}

/// The whole tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagEditorView {
    /// `false` means read-only (a non-local volume).
    pub editable: bool,
    pub read_only_reason: Option<ReadOnlyReason>,
    /// Every field in `TagField::ALL` order except Title and Track, which the track list carries.
    pub shared: Vec<SharedRow>,
    /// The supported audio files, in the session's current order.
    pub tracks: Vec<TrackRow>,
    pub cover: CoverSummary,
    /// The union of the files' unmanaged text keys, sorted.
    pub extras: Vec<String>,
    /// Edited tags differ from the ones on disk.
    pub dirty: bool,
    /// The fields whose edited values still carry diacritics they must not.
    pub diacritics: Vec<TagField>,
    /// How many files carry a Date the writer would refuse (an MP3 whose TDAT reads as month 20):
    /// flagged on open, so the user corrects it before writing.
    pub bad_dates: u32,
    /// The order the tracks are in. Set by the session; `None` straight out of `build`.
    pub sort: Option<TrackSort>,
    /// An edit step can be undone (⌘Z); set by the session.
    pub can_undo_edit: bool,
    /// An undone edit step can be redone (⇧⌘Z); set by the session.
    pub can_redo_edit: bool,
    /// How many tracks `shared` covers (the Tracks pane's selection, readable or not); `None` for every track.
    pub scope_count: Option<u32>,
    /// When the session built this view, under its lock: a later view has a higher number, so the UI keeps the
    /// newest by the backend's order, never by the order it asked in. Set by the session; 0 straight out of `build`.
    pub view_seq: u64,
}

/// What the session's rules say about the window, apart from the files.
#[derive(Debug, Clone, Copy)]
pub(super) struct Access {
    pub editable: bool,
    pub reason: Option<ReadOnlyReason>,
}

/// The view for `tracks` (path and entry, in the session's current order). Unreadable files
/// are listed but count in no summary. Hashes no cover: each file's key comes from its read.
/// `scope` (some of `tracks`) narrows the shared rows and their choices alone; `None` is every track.
pub(super) fn build(access: Access, tracks: &[(&Path, &Entry)], scope: Option<&[(&Path, &Entry)]>) -> TagEditorView {
    let loaded: Vec<&Loaded> = tracks.iter().filter_map(|(_, entry)| entry.loaded()).collect();
    let edited: Vec<FileTags> = loaded.iter().map(|file| file.edited.clone()).collect();
    let in_scope: Cow<'_, [FileTags]> = match scope {
        None => Cow::Borrowed(&edited),
        Some(scoped) => scoped
            .iter()
            .filter_map(|(_, entry)| entry.loaded())
            .map(|file| file.edited.clone())
            .collect(),
    };
    let shared = shared_view(&in_scope);
    let dirty = loaded.iter().any(|file| file.changed());
    TagEditorView {
        editable: access.editable,
        read_only_reason: access.reason,
        shared: TagField::ALL
            .iter()
            .filter(|field| !matches!(field, TagField::Title | TagField::Track))
            .filter_map(|field| shared.get(field).map(|value| (*field, value.clone())))
            .map(|(field, value)| shared_row(&in_scope, field, value))
            .collect(),
        tracks: tracks.iter().map(|(path, entry)| track_row(path, entry)).collect(),
        cover: cover_summary(&loaded),
        extras: edited
            .iter()
            .flat_map(|tags| tags.extras.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        dirty,
        diacritics: edited
            .iter()
            .flat_map(fields_with_diacritics)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        bad_dates: count_bad_dates(tracks),
        sort: None,
        can_undo_edit: false,
        can_redo_edit: false,
        scope_count: scope.map(|scoped| u32::try_from(scoped.len()).unwrap_or(u32::MAX)),
        view_seq: 0,
    }
}

/// The `tracks` at `scope` (the Album pane's, `TagSession::view_scope`), in their order; `None` for every track,
/// and so is a scope none of whose tracks is still listed (a reload dropped them).
pub(super) fn scoped_tracks<'a>(
    tracks: &[(&'a Path, &'a Entry)],
    scope: Option<&HashSet<PathBuf>>,
) -> Option<Vec<(&'a Path, &'a Entry)>> {
    let scope = scope?;
    let scoped: Vec<_> = tracks
        .iter()
        .copied()
        .filter(|(path, _)| scope.contains(*path))
        .collect();
    (!scoped.is_empty()).then_some(scoped)
}

/// One shared row; a «multiple values» one lists up to `CHOICE_LIMIT` distinct values and counts the rest.
fn shared_row(edited: &[FileTags], field: TagField, value: SharedValue) -> SharedRow {
    let mut choices = match value {
        SharedValue::Multiple => distinct_values(edited, field, usize::MAX),
        SharedValue::Empty | SharedValue::Same(_) => Vec::new(),
    };
    let choices_more = u32::try_from(choices.len().saturating_sub(CHOICE_LIMIT)).unwrap_or(u32::MAX);
    choices.truncate(CHOICE_LIMIT);
    SharedRow {
        field,
        value,
        choices,
        choices_more,
    }
}

/// The readable files whose edited Date is one the writer refuses (`cmdr_tags::date_is_writable`, on the first value, as the writer reads it).
fn count_bad_dates(tracks: &[(&Path, &Entry)]) -> u32 {
    let bad = tracks
        .iter()
        .filter(|(path, entry)| {
            let Some(loaded) = entry.loaded() else { return false };
            let Some(format) = path.file_name().and_then(|name| format_of(&name.to_string_lossy())) else {
                return false;
            };
            loaded
                .edited
                .fields
                .get(&TagField::Date)
                .and_then(|values| values.first())
                .is_some_and(|value| !date_is_writable(format, value))
        })
        .count();
    u32::try_from(bad).unwrap_or(u32::MAX)
}

/// The file's name as the session names it: its path below the session's folder (`Disc 1/01.mp3`
/// for a track of a folder), so tracks of different subfolders tell apart.
pub(super) fn file_name_of(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .ok()
        .or_else(|| path.file_name().map(Path::new))
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn track_row(path: &Path, entry: &Entry) -> TrackRow {
    let base = TrackRow {
        path: path.to_string_lossy().into_owned(),
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        track: None,
        title: None,
        duration_ms: None,
        changed: false,
        read_error: None,
    };
    match entry {
        Entry::Unreadable(code) => TrackRow {
            read_error: Some(*code),
            ..base
        },
        Entry::Loaded(loaded) => TrackRow {
            track: first_value(&loaded.edited, TagField::Track),
            title: first_value(&loaded.edited, TagField::Title),
            duration_ms: loaded.duration_ms,
            changed: loaded.changed(),
            ..base
        },
    }
}

fn first_value(tags: &FileTags, field: TagField) -> Option<String> {
    tags.fields.get(&field).and_then(|values| values.first()).cloned()
}

fn cover_summary(files: &[&Loaded]) -> CoverSummary {
    let Some(first) = files.first() else {
        return CoverSummary::Empty;
    };
    if files.iter().any(|file| !same_cover(file, first)) {
        return CoverSummary::Multiple;
    }
    match &first.edited.cover {
        None => CoverSummary::Empty,
        Some(cover) => {
            let (width, height) = pixel_size(cover).unzip();
            CoverSummary::Same {
                mime: cover.mime.clone(),
                bytes: cover.data.len(),
                width,
                height,
                key: format!("{:016x}", first.cover_key.unwrap_or_else(|| cover_key(cover))),
            }
        }
    }
}

/// The cover every file shares (their edited tags); `None` when there are no files, none has a
/// cover, or the covers differ.
pub(super) fn common_cover<'a>(files: &[&'a Loaded]) -> Option<&'a Cover> {
    let first = files.first()?;
    files
        .iter()
        .all(|file| same_cover(file, first))
        .then_some(first.edited.cover.as_ref())?
}

/// Whether two files' edited covers are equal: both absent, or the same MIME and size and
/// either one shared buffer or the same key (`Loaded::cover_key`), so no bytes are compared.
fn same_cover(a: &Loaded, b: &Loaded) -> bool {
    match (&a.edited.cover, &b.edited.cover) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.mime == y.mime
                && x.data.len() == y.data.len()
                && (Arc::ptr_eq(&x.data, &y.data) || (a.cover_key.is_some() && a.cover_key == b.cover_key))
        }
        _ => false,
    }
}

/// The key of a cover: a hash of its bytes, stable within the process (only compared, never
/// stored). Taken once per read (`session::read_entry`) or new cover (`session::store`), never
/// per view: 22 tracks with a 2 MiB cover each are 44 MiB to hash.
pub(super) fn cover_key(cover: &Cover) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    #[cfg(test)]
    HASHES.with(|count| count.set(count.get() + 1));
    let mut hasher = DefaultHasher::new();
    hasher.write(&cover.data);
    hasher.finish()
}

/// Covers already keyed, by their shared buffer, so a cover reached again isn't hashed again: an
/// edit hands covers on as shared buffers, so only a cover new to the session is hashed, once
/// however many files it goes to.
#[derive(Default)]
pub(super) struct CoverKeys {
    known: Vec<(Cover, u64)>,
}

impl CoverKeys {
    pub(super) fn remember(&mut self, cover: &Option<Cover>, key: Option<u64>) {
        if let (Some(cover), Some(key)) = (cover, key)
            && self.find(cover).is_none()
        {
            self.known.push((cover.clone(), key));
        }
    }

    fn find(&self, cover: &Cover) -> Option<u64> {
        self.known
            .iter()
            .find(|(known, _)| known.mime == cover.mime && Arc::ptr_eq(&known.data, &cover.data))
            .map(|(_, key)| *key)
    }

    pub(super) fn key_of(&mut self, cover: &Option<Cover>) -> Option<u64> {
        let cover = cover.as_ref()?;
        let key = self.find(cover).unwrap_or_else(|| cover_key(cover));
        self.remember(&Some(cover.clone()), Some(key));
        Some(key)
    }
}

// How many covers this thread hashed so far: per thread, so tests running side by side in one
// process (plain `cargo test`) don't count each other's.
#[cfg(test)]
thread_local! {
    static HASHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn cover_hashes() -> usize {
    HASHES.with(std::cell::Cell::get)
}

/// The image's size from its header. A decoder that panics on a hostile image costs the
/// size, not the app.
pub(super) fn pixel_size(cover: &Cover) -> Option<(u32, u32)> {
    crate::crash_reporter::contain_panics(|| {
        image::ImageReader::new(std::io::Cursor::new(&cover.data[..]))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()
    })
    .flatten()
}
