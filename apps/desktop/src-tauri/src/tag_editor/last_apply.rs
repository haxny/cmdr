//! The last tag write per folder, which "Undo tag changes" (⌘Z) reverses. One write is every
//! apply of one Write press: the first and, after "Write anyway", the one for the files that
//! would lose data. Kept in memory for the app's lifetime, keyed by the folder, so a Tags window
//! opened later over the same folder can still undo it; the window only mirrors it
//! (`last_tag_apply`). The undo store's records stay the truth of what an undo may touch (each
//! sealed to exactly the files written, refused for a file changed since); this only says which
//! records make the last write.
//!
//! Lock order: under the undo store (`undo.rs`), never the other way round; nothing is taken
//! under this lock.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use super::apply::TagApplyReport;
use super::session::TagEditorError;
use crate::ignore_poison::IgnorePoison;
use crate::multi_rename::session::files_of;

/// One apply Undo can reverse, and how many files it wrote (or still has to put back).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UndoableApply {
    pub apply_id: String,
    pub written: u32,
}

/// The last write of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LastTagApply {
    /// In the order they were written; empty when none kept its old tags.
    pub applies: Vec<UndoableApply>,
    /// Some files of this write were written without their old tags kept: Undo can't reach them.
    pub partial: bool,
}

/// A folder as Multi-Rename names it: its volume and its path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FolderKey {
    volume_id: String,
    dir: PathBuf,
}

impl FolderKey {
    pub(crate) fn new(volume_id: impl Into<String>, dir: PathBuf) -> Self {
        Self {
            volume_id: volume_id.into(),
            dir,
        }
    }
}

/// The last write of every folder written this run, at most `cap` folders: the undo store keeps
/// no more applies than that (`UndoLimits::max_applies`), so a folder written longer ago than
/// the last `cap` writes has had its record pruned anyway.
#[derive(Debug)]
pub(crate) struct LastApplies {
    /// Each folder's last write, with when it was written (a sequence number).
    by_folder: HashMap<FolderKey, (u64, LastTagApply)>,
    written: u64,
    cap: usize,
}

impl Default for LastApplies {
    fn default() -> Self {
        Self::with_cap(cmdr_tags::UndoLimits::default().max_applies)
    }
}

impl LastApplies {
    pub(crate) fn with_cap(cap: usize) -> Self {
        Self {
            by_folder: HashMap::new(),
            written: 0,
            cap: cap.max(1),
        }
    }

    /// Takes in an apply's report. A write that wrote nothing leaves the last one (there's
    /// nothing newer to protect); `continues` (a "Write anyway" after a press that wrote) adds
    /// to it, anything else replaces it.
    pub(crate) fn after_apply(&mut self, key: FolderKey, report: &TagApplyReport, continues: bool) {
        if !continues && files_written(report) == 0 {
            return;
        }
        let before = if continues {
            self.by_folder.remove(&key).map(|(_, last)| last)
        } else {
            None
        };
        match next_last(before, report) {
            Some(next) => {
                self.written += 1;
                self.by_folder.insert(key, (self.written, next));
                self.keep_cap();
            }
            None => {
                self.by_folder.remove(&key);
            }
        }
    }

    /// Drops the folders written longest ago past `cap`.
    fn keep_cap(&mut self) {
        while self.by_folder.len() > self.cap {
            let oldest = self
                .by_folder
                .iter()
                .min_by_key(|(_, (written, _))| *written)
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => self.by_folder.remove(&key),
                None => return,
            };
        }
    }

    /// Takes `apply_id` out: its undo record is gone (the store pruned it), so ⌘Z mustn't offer
    /// it again. A folder whose last apply went is forgotten.
    pub(crate) fn forget(&mut self, apply_id: &str) {
        self.after_undo(apply_id, 0);
    }

    /// Takes in an undo of `apply_id`: gone when it put every file back, else kept with the
    /// files it left (it stays undoable). A folder whose last apply went is forgotten.
    pub(crate) fn after_undo(&mut self, apply_id: &str, left: usize) {
        let Some((key, (_, last))) = self
            .by_folder
            .iter_mut()
            .find(|(_, (_, last))| last.applies.iter().any(|apply| apply.apply_id == apply_id))
        else {
            return;
        };
        if left == 0 {
            last.applies.retain(|apply| apply.apply_id != apply_id);
        } else if let Some(apply) = last.applies.iter_mut().find(|apply| apply.apply_id == apply_id) {
            apply.written = u32::try_from(left).unwrap_or(u32::MAX);
        }
        if last.applies.is_empty() {
            let key = key.clone();
            self.by_folder.remove(&key);
        }
    }

    pub(crate) fn get(&self, key: &FolderKey) -> Option<LastTagApply> {
        self.by_folder.get(key).map(|(_, last)| last.clone())
    }
}

/// The last write after `report`: this apply (when it wrote and kept the old tags) after
/// `before`'s. `None` when nothing is left to undo or to warn about.
fn next_last(before: Option<LastTagApply>, report: &TagApplyReport) -> Option<LastTagApply> {
    let wrote = files_written(report) > 0;
    let mut next = before.unwrap_or(LastTagApply {
        applies: Vec::new(),
        partial: false,
    });
    match &report.apply_id {
        Some(apply_id) if wrote && !report.undo_unavailable => next.applies.push(UndoableApply {
            apply_id: apply_id.clone(),
            written: u32::try_from(files_written(report)).unwrap_or(u32::MAX),
        }),
        _ => {}
    }
    next.partial |= report.undo_unavailable && wrote;
    // Kept with no applies when the write can't be undone at all, so a "Write anyway" after it is
    // marked partial.
    (!next.applies.is_empty() || next.partial).then_some(next)
}

/// The files an apply wrote: its tracks and its side files beside the book.
fn files_written(report: &TagApplyReport) -> usize {
    report.written.len() + report.side_files_written.len()
}

static LAST: LazyLock<Mutex<LastApplies>> = LazyLock::new(|| Mutex::new(LastApplies::default()));

/// The folder of Multi-Rename session `session_id`, or `NoSession`: the one its tracks share, so two
/// books opened from the same parent folder keep their own last write.
pub(crate) fn key_of(session_id: &str) -> Result<FolderKey, TagEditorError> {
    let files = files_of(session_id).map_err(|_| TagEditorError::NoSession)?;
    let dir = super::cover::tracks_folder(&files.dir, files.names.iter());
    Ok(FolderKey::new(files.volume_id, dir))
}

/// Records an apply of session `session_id`'s folder. Call it under the undo store.
pub(crate) fn record(key: FolderKey, report: &TagApplyReport, continues: bool) {
    LAST.lock_ignore_poison().after_apply(key, report, continues);
}

/// Records an undo of `apply_id` that left `left` files unrestored. Call it under the undo store.
pub(crate) fn undone(apply_id: &str, left: usize) {
    LAST.lock_ignore_poison().after_undo(apply_id, left);
}

/// Forgets `apply_id`, whose undo record is gone. Call it under the undo store.
pub(crate) fn forget(apply_id: &str) {
    LAST.lock_ignore_poison().forget(apply_id);
}

/// The last write of session `session_id`'s folder, if there is one.
pub(crate) fn last_of(session_id: &str) -> Result<Option<LastTagApply>, TagEditorError> {
    let key = key_of(session_id)?;
    Ok(LAST.lock_ignore_poison().get(&key))
}
