//! Writing a tag session to disk: the old tags are recorded first, each file is written on
//! its own, and every file that isn't written is reported by why.
//!
//! Flow: the changed files (minus the user's skips) are copied out of the session; one whose
//! fingerprint moved since the session read it is `changedOnDisk` and never written. The rest
//! are recorded in the undo store BEFORE any write, then written one by one, each re-checked
//! right before its write. A `WouldDrop` is `wouldDrop` (the user decides), any other error
//! `failed`. The record is narrowed to the files written, or forgotten when none was; written
//! files become the session's disk copy, and Multi-Rename's media cache reads them again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use cmdr_tags::{ApplyId, ContentKind, FileTags, TagError, UndoStore, WriteOptions, write_tags};

use super::session::{Fingerprint, Pending, TagEditorError, mark_written, pending};
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
    /// Not written, with why.
    pub failed: Vec<FailedRow>,
    /// Not written: something else changed the file since the session read it.
    pub changed_on_disk: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WouldDropRow {
    pub path: String,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedRow {
    pub path: String,
    pub error: TagError,
}

/// Writes the session's changed files. See the module doc for the flow. Blocking file work:
/// call it off the IPC thread.
pub fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session_id: &str,
    req: ApplyRequest,
) -> Result<TagApplyReport, TagEditorError> {
    super::undo::with_store(app, |store| apply_with(store, session_id, req))
}

/// [`apply`] against `store`. The caller holds the store for the whole call (one writer).
pub(super) fn apply_with(
    store: &UndoStore,
    session_id: &str,
    req: ApplyRequest,
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
    let paths: Vec<PathBuf> = ready.iter().map(|file| file.path.clone()).collect();
    // `record` parses each file with lofty.
    contain_panics(|| store.record(&id, &paths))
        .unwrap_or_else(|| Err(panicked()))
        .map_err(|error| TagEditorError::Undo { error })?;

    let opts = WriteOptions {
        kind: req.kind,
        track_total: None,
        disc_total: None,
        allow_frame_loss: req.allow_frame_loss,
    };
    let written = write_each(ready, &opts, &mut report);
    report.written = written.iter().map(|(path, ..)| shown(path)).collect();
    let written_paths: Vec<PathBuf> = written.iter().map(|(path, ..)| path.clone()).collect();
    narrow_record(store, &id, &written_paths);
    if !written_paths.is_empty() {
        report.apply_id = Some(id.0);
    }
    mark_written(session_id, written);
    reread_media(&target.volume_id, &written_paths);
    Ok(report)
}

/// Writes each file, re-checking its fingerprint right before. Files not written land in
/// `report`; the written ones come back with their tags and post-write fingerprint.
fn write_each(
    ready: Vec<Pending>,
    opts: &WriteOptions,
    report: &mut TagApplyReport,
) -> Vec<(PathBuf, FileTags, Option<Fingerprint>)> {
    let mut written = Vec::new();
    for file in ready {
        if !Fingerprint::holds(file.fingerprint.as_ref(), &file.path) {
            report.changed_on_disk.push(shown(&file.path));
            continue;
        }
        let outcome = contain_panics(|| write_tags(&file.path, &file.tags, opts)).unwrap_or_else(|| Err(panicked()));
        match outcome {
            Ok(()) => {
                let fingerprint = Fingerprint::of(&file.path);
                written.push((file.path, file.tags, fingerprint));
            }
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

/// Keeps in the record only the files written, so an undo touches exactly those; forgets it
/// when none was. The files are written by now, so a store that refuses is logged, not
/// returned: the record then still names the unwritten files too, whose old tags are their
/// current ones.
fn narrow_record(store: &UndoStore, id: &ApplyId, written: &[PathBuf]) {
    let outcome = if written.is_empty() {
        store.forget(id)
    } else {
        store.retain(id, written)
    };
    if let Err(error) = outcome {
        log::warn!(target: LOG_TARGET, "Couldn't narrow the undo record to the files written: {error:?}");
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
fn panicked() -> TagError {
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
