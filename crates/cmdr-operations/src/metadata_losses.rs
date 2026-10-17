//! What an operation couldn't carry of its sources' metadata, counted per class
//! as items land, for the one calm note on its completion.
//!
//! **Counted, never listed.** A fixed array of counters on
//! `WriteOperationState`, so a 100,000-file copy onto a phone costs a few bytes,
//! not a per-file list. Only an item that LANDED counts, and only for a class it
//! had something worth keeping in (`CarriedMetadata::worth_keeping`): a plain
//! file copied anywhere reports nothing.
//!
//! **Who counts.** The cross-volume engine, from the destination's
//! `MetadataCaps` against the source's plus what the destination's write
//! refused at runtime (`StreamWritten::refused`): a file at landing
//! (`transfer/volume/stream_pipe.rs`, `sequential_extract.rs`), a folder when its
//! dates and xattrs go on (`transfer/volume/folder_metadata.rs`). The native engine keeps
//! everything local disk can hold and counts nothing. The contract:
//! `transfer/volume/DETAILS.md` § "Copies keep the source's metadata".

use std::sync::atomic::{AtomicU32, Ordering};

use cmdr_fs::metadata::MetadataClasses;

use super::types::{LostMetadata, LostMetadataClass, MetadataLeftBehind};
use cmdr_fs::volume::{Volume, VolumeReadStream};

/// Every class in the order the completion event lists them.
const CLASSES: [LostMetadataClass; 5] = [
    LostMetadataClass::CreationDates,
    LostMetadataClass::Permissions,
    LostMetadataClass::LockedAndHidden,
    LostMetadataClass::ExtendedAttributes,
    LostMetadataClass::FolderDates,
];

/// What a FILE copied from `source` onto `dest` is about to arrive without, of
/// what it had worth keeping, read off its open `stream` before the
/// destination consumes it.
///
/// Asks the stream only about the classes the source reports and the
/// destination can't store, and nothing at all for a pair that loses nothing
/// (invariant 10): a copy onto a destination that keeps everything never pays
/// for the question. The answer rides what the open already holds
/// (`VolumeReadStream::metadata_outline`: a local file's xattrs by name only).
pub(crate) fn file_losses(source: &dyn Volume, dest: &dyn Volume, stream: &dyn VolumeReadStream) -> MetadataClasses {
    let at_risk = dest.metadata_caps().file_classes_at_risk_from(source.metadata_caps());
    if at_risk.is_empty() {
        return MetadataClasses::NONE;
    }
    stream
        .metadata_outline(at_risk)
        .worth_keeping(stream.modified_at(), cmdr_fs::metadata::default_file_mode())
}

/// What one FOLDER arrived without, of what it had worth keeping.
// DEFAULT-OK: nothing lost.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FolderLosses {
    pub(crate) created: bool,
    pub(crate) modified: bool,
    pub(crate) xattrs: bool,
    pub(crate) flags: bool,
}

/// Per-class counts of items that arrived without something worth keeping.
// DEFAULT-OK: all zero is "nothing left behind yet", the honest start.
#[derive(Default)]
pub(crate) struct MetadataLosses {
    counts: [AtomicU32; CLASSES.len()],
}

impl MetadataLosses {
    /// A FILE landed without `lost` (engine classes, already narrowed to what
    /// it had worth keeping).
    pub(crate) fn note_file(&self, lost: MetadataClasses) {
        for (class, engine) in [
            (LostMetadataClass::CreationDates, MetadataClasses::CREATED),
            (LostMetadataClass::Permissions, MetadataClasses::MODE),
            (LostMetadataClass::LockedAndHidden, MetadataClasses::FLAGS),
            (LostMetadataClass::ExtendedAttributes, MetadataClasses::XATTRS),
        ] {
            if lost.contains(engine) {
                self.bump(class);
            }
        }
    }

    /// A FOLDER the operation created arrived without its creation date, its
    /// modification date, its xattrs, its flags, or any mix of them.
    pub(crate) fn note_folder(&self, lost: FolderLosses) {
        if lost.created {
            self.bump(LostMetadataClass::CreationDates);
        }
        if lost.xattrs {
            self.bump(LostMetadataClass::ExtendedAttributes);
        }
        if lost.modified {
            self.bump(LostMetadataClass::FolderDates);
        }
        if lost.flags {
            self.bump(LostMetadataClass::LockedAndHidden);
        }
    }

    /// What the completion event carries: `None` when nothing was left behind,
    /// and one `info!` line naming the classes and counts when something was.
    pub(crate) fn left_behind(&self, operation_id: &str) -> Option<MetadataLeftBehind> {
        let classes: Vec<LostMetadata> = CLASSES
            .iter()
            .zip(&self.counts)
            .filter_map(|(class, count)| {
                let item_count = count.load(Ordering::Relaxed);
                (item_count > 0).then_some(LostMetadata {
                    class: *class,
                    item_count,
                })
            })
            .collect();
        if classes.is_empty() {
            return None;
        }
        log::info!(target: "transfer", "op={operation_id}: the destination couldn't keep {classes:?}");
        Some(MetadataLeftBehind { classes })
    }

    fn bump(&self, class: LostMetadataClass) {
        let index = CLASSES.iter().position(|known| *known == class).unwrap_or_default();
        // Saturating: a count past `u32::MAX` items still reads as "a lot".
        let _ = self.counts[index].try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1));
    }
}

#[cfg(test)]
#[path = "metadata_losses_tests.rs"]
mod tests;
