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
//! A folder that arrives without a date its source had (the destination can't
//! hold it, or refused it) is counted for the loss note
//! (`write_operations/metadata_losses.rs`).
//!
//! ❌ Not the extract quarantine: a folder gets that the moment the walk creates
//! it (`extract_quarantine.rs`), since this pass skips a folder with no source
//! date and every folder of a cancelled subtree.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataClasses};

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

    /// Puts every noted folder's dates on `dest_volume`, deepest first, and
    /// counts each date a folder arrives without.
    ///
    /// Call it only once the subtree landed in full. Best effort: a refusal is
    /// a `log::warn!` and the copy stands; a destination that can hold none of
    /// it (`MetadataReport::holds_none`) gets asked no more, though every
    /// folder still counts what it lost.
    pub(super) async fn stamp(self, dest_volume: &Arc<dyn Volume>, state: &WriteOperationState) {
        let mut asking = true;
        for (dest, source) in self.folders {
            if is_cancelled(&state.intent) {
                return;
            }
            let outcome = if asking {
                stamp_one(dest_volume, &dest, source).await
            } else {
                Stamped::NotAsked
            };
            asking = !matches!(outcome, Stamped::HeldNone);
            note_losses(dest_volume, source, outcome, state);
        }
    }
}

/// What stamping one folder did.
enum Stamped {
    /// The destination answered, class by class.
    Answered {
        created: ClassOutcome,
        modified: ClassOutcome,
    },
    /// The destination can hold none of a folder's dates; ask it no more.
    HeldNone,
    /// An earlier folder's [`Stamped::HeldNone`] ended the asking.
    NotAsked,
}

/// Counts what `source` had worth keeping that its folder arrived without:
/// what the destination's caps say it can't hold, or what it refused.
fn note_losses(
    dest_volume: &Arc<dyn Volume>,
    source: SourceFolderDates,
    outcome: Stamped,
    state: &WriteOperationState,
) {
    let caps = dest_volume.metadata_caps();
    let (created, modified) = match outcome {
        Stamped::Answered { created, modified } => (created, modified),
        Stamped::HeldNone | Stamped::NotAsked => (ClassOutcome::NotApplicable, ClassOutcome::NotApplicable),
    };
    let created_worth_keeping = match (source.created_at, source.modified_at) {
        (Some(created), Some(modified)) => created < modified,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let lost_created = created_worth_keeping
        && (!caps.folder_stores.contains(MetadataClasses::CREATED) || created == ClassOutcome::Refused);
    let lost_modified = source.modified_at.is_some() && (!caps.folder_dates || modified == ClassOutcome::Refused);
    state.metadata_losses.note_folder(lost_created, lost_modified);
}

/// Stamps one folder.
async fn stamp_one(dest_volume: &Arc<dyn Volume>, dest: &Path, source: SourceFolderDates) -> Stamped {
    let at = |secs: u64| UNIX_EPOCH + Duration::from_secs(secs);
    let carried = CarriedMetadata {
        created: source.created_at.map(at),
        ..CarriedMetadata::default()
    };
    let report = dest_volume
        .apply_metadata(dest, &carried, source.modified_at.map(at))
        .await;
    if report.holds_none() {
        return Stamped::HeldNone;
    }
    if report.any_refused() {
        log::warn!(
            target: "transfer",
            "folder metadata: {} keeps some of the copy's own, the destination refused the source's: {report:?}",
            dest.display()
        );
    }
    Stamped::Answered {
        created: report.created,
        modified: report.modified,
    }
}
