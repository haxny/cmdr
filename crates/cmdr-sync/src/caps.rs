//! What a volume can do, as far as a sync cares.

use cmdr_fs::volume::{BackendKind, Volume};
use serde::{Deserialize, Serialize};

/// Per-transport facts no `Volume` method reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackendFacts {
    /// Whether `A.txt` and `a.txt` are two different files here.
    pub case_sensitive: bool,
    /// The coarsest step the stored modification time moves in, in seconds.
    pub mtime_precision_secs: u64,
}

/// The facts of one transport. Evidence per row: `DETAILS.md`.
pub fn backend_facts(kind: BackendKind) -> BackendFacts {
    let case_sensitive = match kind {
        BackendKind::Local => !cfg!(target_os = "macos"),
        BackendKind::Smb | BackendKind::Mtp | BackendKind::Adb => false,
        BackendKind::Sftp
        | BackendKind::Webdav
        | BackendKind::S3
        | BackendKind::Nfs
        | BackendKind::Archive
        | BackendKind::GitPortal
        | BackendKind::Restic => true,
    };
    let mtime_precision_secs = if kind == BackendKind::Archive { 2 } else { 1 };
    BackendFacts {
        case_sensitive,
        mtime_precision_secs,
    }
}

/// What one side of a sync can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncCaps {
    /// Whether `A.txt` and `a.txt` are two different files here.
    pub case_sensitive: bool,
    /// Whether a name matches in any Unicode form (NFC or NFD).
    pub names_in_any_unicode_form: bool,
    /// The coarsest step the stored modification time moves in, in seconds.
    pub mtime_precision_secs: u64,
    /// Whether the volume takes writes.
    pub can_write: bool,
    /// Whether removed items can go to a trash.
    pub has_trash: bool,
    /// Whether a rename can replace an existing target atomically.
    pub atomic_replace: bool,
    /// Whether file bytes can be read (for a content compare).
    pub can_read_content: bool,
    /// Whether a rename may turn into a copy plus delete (cross-device).
    pub renames_can_copy: bool,
}

impl SyncCaps {
    /// Reads the capabilities of `volume`.
    pub fn of(volume: &dyn Volume) -> Self {
        let facts = backend_facts(volume.backend_kind());
        Self {
            case_sensitive: facts.case_sensitive,
            names_in_any_unicode_form: volume.matches_names_in_any_unicode_form(),
            mtime_precision_secs: facts.mtime_precision_secs,
            can_write: volume.is_writable(),
            has_trash: volume.has_trash(),
            atomic_replace: volume.supports_atomic_replace_rename(),
            can_read_content: volume.supports_export(),
            renames_can_copy: volume.renames_can_copy(),
        }
    }
}
