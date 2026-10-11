//! Giving the folders a cross-volume copy created their source folders' dates,
//! modification AND creation, once everything inside them has landed.
//!
//! Writing a child bumps its folder's date on every real store, so a folder can
//! only keep its source's date if it's set AFTER its last child: the merge walk
//! notes each folder it created as it leaves it (post-order), and the copy
//! stamps the whole list once the subtree's leaves have drained. A subtree that
//! failed or was cancelled is never stamped: a folder holding part of its
//! contents must not claim the date of the whole. The set comes from the
//! listing entry the walk already holds (the scan's stat for a top-level
//! folder), so it costs no round trip. `DETAILS.md` § "Copies keep the source's
//! metadata".
//!
//! ❌ Not the extract quarantine: a folder gets that the moment the walk creates
//! it (`extract_quarantine.rs`), since this pass skips a folder with no source
//! date and every folder of a cancelled subtree.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use cmdr_fs::metadata::CarriedMetadata;

use super::super::super::state::{WriteOperationState, is_cancelled};
use crate::file_system::volume::Volume;

/// A source folder's own dates (Unix seconds), as the listing entry or the
/// scan's stat reported them. `None` is a date the source didn't carry.
// DEFAULT-OK: both "the source carried no date", which leaves the folder the
// store's own; nothing reads it as a claim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SourceFolderDates {
    pub modified_at: Option<u64>,
    pub created_at: Option<u64>,
}

impl SourceFolderDates {
    /// What a listing entry says about its folder.
    pub(super) fn of_entry(entry: &crate::file_system::listing::FileEntry) -> Self {
        Self {
            modified_at: entry.modified_at,
            created_at: entry.created_at,
        }
    }

    fn is_empty(self) -> bool {
        self.modified_at.is_none() && self.created_at.is_none()
    }
}

/// The folders one top-level source's walk created, each with its source
/// folder's dates, in the order they're stamped.
// DEFAULT-OK: an empty list is a walk that hasn't created a folder yet.
#[derive(Default)]
pub(super) struct FolderMetadata {
    /// In the order the walk LEFT them, which puts every folder after the
    /// folders inside it, and the top-level one last.
    folders: Vec<(PathBuf, SourceFolderDates)>,
}

impl FolderMetadata {
    /// A folder the walk created at `dest` and has just finished filling. A
    /// source that carried no date at all (an S3 prefix) leaves it the
    /// store's own.
    pub(super) fn note_filled(&mut self, dest: PathBuf, source: SourceFolderDates) {
        if !source.is_empty() {
            self.folders.push((dest, source));
        }
    }

    /// Puts every noted folder's dates on `dest_volume`, deepest first.
    ///
    /// Call it only once the subtree landed in full. Best effort: a refusal is
    /// a `log::warn!` and the copy stands; a destination that can hold none of
    /// it (`MetadataReport::holds_none`) ends the pass at its first answer.
    pub(super) async fn stamp(self, dest_volume: &Arc<dyn Volume>, state: &WriteOperationState) {
        for (dest, source) in self.folders {
            if !stamp_one(dest_volume, &dest, source, state).await {
                return;
            }
        }
    }
}

/// Stamps one folder. `false` means stop: the operation was cancelled, or the
/// destination can't hold a folder's dates at all.
async fn stamp_one(
    dest_volume: &Arc<dyn Volume>,
    dest: &Path,
    source: SourceFolderDates,
    state: &WriteOperationState,
) -> bool {
    if is_cancelled(&state.intent) {
        return false;
    }
    let at = |secs: u64| UNIX_EPOCH + Duration::from_secs(secs);
    let carried = CarriedMetadata {
        created: source.created_at.map(at),
        ..CarriedMetadata::default()
    };
    let report = dest_volume
        .apply_metadata(dest, &carried, source.modified_at.map(at))
        .await;
    if report.holds_none() {
        return false;
    }
    if report.any_refused() {
        log::warn!(
            target: "transfer",
            "folder metadata: {} keeps some of the copy's own, the destination refused the source's: {report:?}",
            dest.display()
        );
    }
    true
}
