//! Writing a tag session to disk: the old tags are recorded first, each file is written on
//! its own, and every file that isn't written is reported by why.
//!
//! Flow: the changed files (minus the user's skips) are copied out of the session; one whose
//! fingerprint moved since the session read it is `changedOnDisk` and never written. The rest
//! are recorded in the undo store BEFORE any write, then written one by one, each re-checked
//! right before its write, and held back as `wouldRemove` when the kind would remove a field
//! from it (an MP3's conductor under Audiobook) that the request doesn't allow (a removal is
//! written only from the list that showed it; an unreadable check holds it back too). A
//! `WouldDrop` is `wouldDrop` (the user decides), any other error
//! `failed`. The record is sealed to the files written, each with its fingerprint right after
//! its write (an undo leaves a file alone once that moves), or forgotten when none was; written
//! files become the session's disk copy, and Multi-Rename's media cache reads them again.
//! With `side_files`, Audiobookshelf's `reader.txt` / `desc.txt` follow a tag write (never alone),
//! from what the files say after it (`side_files.rs`); a different one is replaced only as
//! `allow_side_files` names it, and only when the write keeps an undo.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use cmdr_tags::{
    ApplyId, ContentKind, FileTags, Fingerprint, KindRemoval, TagError, UndoStore, WriteOptions, write_tags,
};

use super::removals::{Removals, removals_of};
use super::session::{Pending, TagEditorError, mark_written, pending};
use super::side_files::{self, BookTags, SideFileSeen, book_of};
use crate::crash_reporter::contain_panics;
use crate::multi_rename::session::media_over;

const LOG_TARGET: &str = "tag_editor::apply";

/// What the user asked to write.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub kind: ContentKind,
    /// Write files whose write would drop items (the user chose "Write anyway").
    pub allow_frame_loss: bool,
    /// Paths the user chose to skip, as the view shows them.
    pub skip: Vec<String>,
    /// Paths (as the view shows them) whose kind removals the user saw in the change list and
    /// confirmed: only these may lose a field the kind removes. Absent means none.
    #[serde(default)]
    pub allow_removals: Vec<String>,
    /// Write Audiobookshelf's `reader.txt` and `desc.txt` beside the book too (`side_files.rs`;
    /// the `multiRename.tagEditor.absSideFiles` setting). Absent means no.
    #[serde(default)]
    pub side_files: bool,
    /// The side-file replacements the change list showed, each with the old text it showed: only
    /// these may replace a side file that says something else. Absent means none.
    #[serde(default)]
    pub allow_side_files: Vec<SideFileSeen>,
}

/// What a write did, per file. Paths are as the view shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagApplyReport {
    /// What undoes this write; `None` when nothing was written.
    pub apply_id: Option<String>,
    pub written: Vec<String>,
    /// Not written: the write would drop these items. The UI asks.
    pub would_drop: Vec<WouldDropRow>,
    /// Not written: the kind would remove a field the request didn't allow. The UI lists it.
    pub would_remove: Vec<WouldRemoveRow>,
    /// Not written, with why.
    pub failed: Vec<FailedRow>,
    /// Not written: something else changed the file since the session read it.
    pub changed_on_disk: Vec<String>,
    /// Files were written, but the undo store couldn't keep their old tags: no undo.
    pub undo_unavailable: bool,
    /// Side files written beside the book (`reader.txt`, `desc.txt`), by path.
    pub side_files_written: Vec<String>,
    /// Side files not replaced: they say something else, and the request didn't name that
    /// replacement. The UI lists them.
    pub side_files_held: Vec<String>,
    /// Side files that couldn't be written.
    pub side_files_failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WouldDropRow {
    pub path: String,
    pub items: Vec<String>,
}

/// A file held back because writing it as the kind would remove a field (no values, ever).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WouldRemoveRow {
    pub path: String,
    /// What the kind would remove; empty when `unknown`.
    pub removes: Vec<KindRemoval>,
    /// The file couldn't be read to tell: it may lose a field, so it isn't written.
    pub unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedRow {
    pub path: String,
    pub error: TagError,
}

/// Writes the session's changed files. See the module doc for the flow. Blocking file work:
/// call it off the IPC thread.
///
/// The folder's last write (`last_apply`) takes the report in: `continues` (a "Write anyway"
/// after a press that wrote) adds to it, anything that wrote replaces it.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session_id: &str,
    req: ApplyRequest,
    continues: bool,
) -> Result<TagApplyReport, TagEditorError> {
    super::undo::with_store(app, |store| {
        let report = apply_with(store, session_id, req)?;
        // The session can't go mid-apply (the window holds it); if it did, the write stands
        // without a ⌘Z later, which is all a missing key costs.
        if let Ok(key) = super::last_apply::key_of(session_id) {
            super::last_apply::record(key, &report, continues);
        }
        Ok(report)
    })
}

/// [`apply`] against `store`. The caller holds the store for the whole call (one writer).
pub(super) fn apply_with(
    store: &UndoStore,
    session_id: &str,
    req: ApplyRequest,
) -> Result<TagApplyReport, TagEditorError> {
    apply_with_before_write(store, session_id, req, &|_| {})
}

/// [`apply_with`], calling `before_write` with each file's path just before its last check
/// and write: the seam a test changes a file through.
pub(super) fn apply_with_before_write(
    store: &UndoStore,
    session_id: &str,
    req: ApplyRequest,
    before_write: &dyn Fn(&Path),
) -> Result<TagApplyReport, TagEditorError> {
    // Compared as the view shows paths, so a file whose name isn't UTF-8 can still be skipped.
    let skip = |path: &Path| req.skip.iter().any(|skipped| *skipped == shown(path));
    let (target, candidates) = pending(session_id, &skip)?;
    let mut report = TagApplyReport::default();
    let (ready, moved): (Vec<Pending>, Vec<Pending>) = candidates
        .into_iter()
        .partition(|file| Fingerprint::holds(file.fingerprint.as_ref(), &file.path));
    report.changed_on_disk = moved.iter().map(|file| shown(&file.path)).collect();
    if ready.is_empty() {
        return Ok(report);
    }

    let id = ApplyId(format!("tags-{}-{session_id}", unix_millis()));
    let written = write_tracks(store, &id, ready, &req, before_write, &mut report)?;
    let written_paths: Vec<PathBuf> = written.iter().map(|(path, _)| path.clone()).collect();
    mark_written(session_id, written);
    reread_media(&target.volume_id, &written_paths);
    // Side files ride along with a tag write, never alone, and say what the files on disk say now.
    if req.side_files && !report.written.is_empty() {
        write_side_files(session_id, &req.allow_side_files, &mut report);
    }
    Ok(report)
}

/// Writes the book's side files after a tag write, from the tracks as they are on disk now (the
/// written ones read again by `mark_written`), so a skipped or held track's edits never reach them.
/// A replacement needs this write's undo (`apply_id`): without one it is held back. The tracks
/// stand whatever happens here.
fn write_side_files(session_id: &str, allowed: &[SideFileSeen], report: &mut TagApplyReport) {
    let book = match book_of(session_id, BookTags::OnDisk) {
        Ok(Some(book)) => book,
        Ok(None) => return,
        Err(error) => {
            log::warn!(target: LOG_TARGET, "Couldn't read the book for its side files after a write: {error:?}");
            return;
        }
    };
    let side = side_files::write(&book.folder, &book.tracks, allowed, report.apply_id.is_some());
    report.side_files_written.clone_from(&side.written);
    report.side_files_held.clone_from(&side.held);
    report.side_files_failed.clone_from(&side.failed);
    side.keep_undo(report.apply_id.as_deref());
}

/// Records `ready`'s old tags under `id`, writes each, and seals the record to the files written
/// (`apply_id`, or `undo_unavailable`). Answers the files written with the tags asked for.
fn write_tracks(
    store: &UndoStore,
    id: &ApplyId,
    ready: Vec<Pending>,
    req: &ApplyRequest,
    before_write: &dyn Fn(&Path),
    report: &mut TagApplyReport,
) -> Result<Vec<(PathBuf, FileTags)>, TagEditorError> {
    let paths: Vec<PathBuf> = ready.iter().map(|file| file.path.clone()).collect();
    // `record` parses each file with lofty.
    contain_panics(|| store.record(id, &paths))
        .unwrap_or_else(|| Err(panicked()))
        .map_err(|error| TagEditorError::Undo { error })?;

    // The totals travel in the tags (`TrackTotal`, `DiscTotal`), so no fallback is passed.
    let opts = WriteOptions {
        kind: req.kind,
        track_total: None,
        disc_total: None,
        allow_frame_loss: req.allow_frame_loss,
    };
    let allowed = |path: &Path| req.allow_removals.iter().any(|allowed| *allowed == shown(path));
    let written = write_each(ready, &opts, &allowed, before_write, report);
    report.written = written.iter().map(|file| shown(&file.path)).collect();
    let sealed: Vec<(PathBuf, Fingerprint)> = written
        .iter()
        .filter_map(|file| Some((file.path.clone(), file.fingerprint.clone()?)))
        .collect();
    if sealed.len() < written.len() {
        // Gone or unreachable right after its write: an undo couldn't tell it's still ours.
        log::warn!(target: LOG_TARGET, "A written file couldn't be stat'ed after its write, so its undo is unavailable");
    }
    if narrow_record(store, id, &sealed) {
        report.apply_id = Some(id.0.clone());
    } else {
        report.undo_unavailable = !written.is_empty();
    }
    Ok(written.into_iter().map(|file| (file.path, file.tags)).collect())
}

/// A file written: its path, the tags asked for, and its fingerprint right after the write
/// (`None` when it couldn't be stat'ed).
struct Written {
    path: PathBuf,
    tags: FileTags,
    fingerprint: Option<Fingerprint>,
}

/// Writes each file, re-checking its fingerprint right before. Files not written land in
/// `report`; the written ones come back with the tags asked for and their new fingerprint.
fn write_each(
    ready: Vec<Pending>,
    opts: &WriteOptions,
    removal_allowed: &dyn Fn(&Path) -> bool,
    before_write: &dyn Fn(&Path),
    report: &mut TagApplyReport,
) -> Vec<Written> {
    let mut written = Vec::new();
    for file in ready {
        before_write(&file.path);
        if !Fingerprint::holds(file.fingerprint.as_ref(), &file.path) {
            report.changed_on_disk.push(shown(&file.path));
            continue;
        }
        if let Some(row) = held_back(&file.path, opts.kind, removal_allowed) {
            report.would_remove.push(row);
            continue;
        }
        let outcome = contain_panics(|| write_tags(&file.path, &file.tags, opts)).unwrap_or_else(|| Err(panicked()));
        match outcome {
            Ok(()) => written.push(Written {
                fingerprint: Fingerprint::of(&file.path),
                path: file.path,
                tags: file.tags,
            }),
            Err(TagError::WouldDrop(items)) => report.would_drop.push(WouldDropRow {
                path: shown(&file.path),
                items,
            }),
            Err(error) => report.failed.push(FailedRow {
                path: shown(&file.path),
                error,
            }),
        }
    }
    written
}

/// The row for a file the kind would remove a field from (or may: an unreadable check) when the
/// request doesn't allow it; `None` when the write may go ahead.
pub(super) fn held_back(
    path: &Path,
    kind: ContentKind,
    removal_allowed: &dyn Fn(&Path) -> bool,
) -> Option<WouldRemoveRow> {
    let removals = removals_of(path, kind);
    if !removals.any() || removal_allowed(path) {
        return None;
    }
    let (removes, unknown) = match removals {
        Removals::Known(removes) => (removes, false),
        Removals::Unknown => (Vec::new(), true),
    };
    Some(WouldRemoveRow {
        path: shown(path),
        removes,
        unknown,
    })
}

/// Seals the record to the files written, each with its fingerprint after its write, so an
/// undo touches exactly those and only while they are still as written; answers whether it
/// can be undone, and forgets the record when nothing was written. A store that can't seal it
/// drops it instead (the full record would also rewrite files this apply never wrote, changed
/// since by someone else perhaps): the write stands, without an undo.
pub(super) fn narrow_record(store: &UndoStore, id: &ApplyId, written: &[(PathBuf, Fingerprint)]) -> bool {
    if written.is_empty() {
        forget_record(store, id);
        return false;
    }
    match store.seal(id, written) {
        Ok(()) => true,
        Err(error) => {
            log::warn!(target: LOG_TARGET, "Couldn't narrow the undo record to the files written, so this write has no undo: {error:?}");
            forget_record(store, id);
            false
        }
    }
}

/// Removes a record; one that stays is only pruned later, so a failure is logged.
fn forget_record(store: &UndoStore, id: &ApplyId) {
    if let Err(error) = store.forget(id) {
        log::warn!(target: LOG_TARGET, "Couldn't remove an undo record: {error:?}");
    }
}

/// Makes every Multi-Rename session over these files' folders read them again, so a
/// `[=tag.*]` preview shows what was just written, not what its cache read before.
pub(super) fn reread_media(volume_id: &str, paths: &[PathBuf]) {
    let mut by_dir: HashMap<&Path, Vec<String>> = HashMap::new();
    for path in paths {
        if let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
            by_dir.entry(dir).or_default().push(name.to_string_lossy().into_owned());
        }
    }
    for (dir, names) in by_dir {
        for media in media_over(volume_id, dir) {
            media.reread(&names);
        }
    }
}

/// A lofty panic, as the file's error.
pub(super) fn panicked() -> TagError {
    TagError::Write("the tag library panicked".to_owned())
}

/// A path as the view shows it (`TrackRow::path`).
pub(super) fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Now in Unix milliseconds, and always past the last answer, so two applies of one session
/// in the same millisecond never share (and so replace) a record.
fn unix_millis() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    let previous = LAST
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| Some(now.max(last + 1)))
        .unwrap_or_else(|last| last);
    now.max(previous + 1)
}
