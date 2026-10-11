//! What a volume can keep and report of a copy's metadata: the one place a
//! backend states it, so the engine knows which classes a copy is about to leave
//! behind, and the conformance layer holds every backend to its claim.

use super::MetadataClasses;

/// Which metadata classes a volume keeps on what a copy writes, and which its
/// read streams report. mtime isn't a class here: every backend keeps a file's
/// (`VolumeReadStream::modified_at`); only a folder's is a capability.
///
/// A claim about the BACKEND, answered without touching a path, like every
/// predicate `capabilities()` folds. ❗ A backend claiming a class it doesn't
/// honor fails `cmdr_fs::volume::conformance::metadata`, and one that keeps a
/// class it doesn't claim makes every copy onto it report a loss that didn't
/// happen.
// DEFAULT-OK: nothing kept and nothing reported, the honest answer of a backend
// that stores no metadata beyond a file's mtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetadataCaps {
    /// What a FILE a copy writes here keeps, through `Volume::write_from_stream`.
    /// No volume claims the user flags yet: they'd go on after the landing,
    /// which the volume engine starts doing in M6
    /// (`docs/specs/transfer-metadata-preservation.md`).
    pub stores: MetadataClasses,
    /// What this volume's read streams report from what their open already
    /// holds (`VolumeReadStream::metadata`). A class not in here is unknown,
    /// never "absent": a copy off this volume never reports it lost.
    pub reports: MetadataClasses,
    /// What `Volume::apply_metadata` keeps by path: a FOLDER's set once its
    /// subtree lands (the copy's one path-based write today).
    pub folder_stores: MetadataClasses,
    /// Whether a folder created here keeps its source's modification date.
    pub folder_dates: bool,
}

impl MetadataCaps {
    /// Keeps and reports nothing beyond a file's mtime: MTP, WebDAV, S3 today.
    pub const NONE: Self = Self {
        stores: MetadataClasses::NONE,
        reports: MetadataClasses::NONE,
        folder_stores: MetadataClasses::NONE,
        folder_dates: false,
    };

    /// Reports a mode and keeps nothing: a source whose files carry a real mode
    /// but nowhere a copy can store anything (an archive, a git snapshot, an
    /// Android phone over ADB).
    pub const MODE_REPORTER: Self = Self {
        reports: MetadataClasses::MODE,
        ..Self::NONE
    };

    /// Keeps and reports every class, on files and folders.
    pub const ALL: Self = Self {
        stores: MetadataClasses::ALL,
        reports: MetadataClasses::ALL,
        folder_stores: MetadataClasses::ALL,
        folder_dates: true,
    };

    /// The classes a FILE copied from `source` onto this volume can't keep,
    /// among those `source` reports: the ones a copy has to ask the source
    /// about, and report when it had one. Empty for every pair that loses
    /// nothing, so such a copy asks nothing.
    pub fn file_classes_at_risk_from(self, source: Self) -> MetadataClasses {
        source.reports.without(self.stores)
    }
}

#[cfg(test)]
#[path = "caps_tests.rs"]
mod tests;
