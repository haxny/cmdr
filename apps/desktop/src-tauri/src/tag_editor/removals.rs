//! What a write as a [`ContentKind`] removes beyond the fields (an MP3's conductor under
//! Audiobook), as one typed answer for the change list and for the write's guard. A file whose
//! removals can't be read is [`Removals::Unknown`], ❌ never an empty list: an unread file may
//! well lose a field, so the list says it couldn't check and the write holds it back unless the
//! user saw that (`ApplyRequest::allow_removals`).

use std::path::Path;

use cmdr_tags::{ContentKind, KindRemoval, removed_by_kind};

use crate::crash_reporter::contain_panics;

const LOG_TARGET: &str = "tag_editor::removals";

/// What writing one file as a kind removes beyond its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Removals {
    /// Read from the file: these (often none).
    Known(Vec<KindRemoval>),
    /// The file couldn't be read: it may lose something.
    Unknown,
}

impl Removals {
    /// Whether the write takes anything away the fields don't show, or may.
    pub(super) fn any(&self) -> bool {
        match self {
            Self::Known(removals) => !removals.is_empty(),
            Self::Unknown => true,
        }
    }
}

/// What writing `path` as `kind` removes. Parses the file (a music write reads nothing).
/// Blocking: ❌ not under a lock.
pub(super) fn removals_of(path: &Path, kind: ContentKind) -> Removals {
    match contain_panics(|| removed_by_kind(path, kind)) {
        Some(Ok(removals)) => Removals::Known(removals),
        Some(Err(_)) | None => {
            log::warn!(target: LOG_TARGET, "Couldn't read what a write kind would remove from a file");
            Removals::Unknown
        }
    }
}
