//! What a path-aware volume lookup answers: which volume serves a path, and
//! whether a route swapped it in.
//!
//! The host's registry (`VolumeManager::resolve` in the app) is what produces a
//! [`ResolvedVolume`]; the type lives here so a consumer that only READS the
//! answer (the listing engine's routing questions, a future non-Tauri host) can
//! name it without reaching into the app.

use super::Volume;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Which route swapped the volume in, when one did.
///
/// Both variants mean the same thing to most readers — "this isn't the drive
/// the caller named, it's a read-only volume mapping a namespace onto it" — so
/// most sites ask [`ResolvedVolume::is_routed`]. Match a variant only where the
/// answer is genuinely about that ONE backend (the archive-edit driver, the
/// archive preview, the routed materialization's error mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutedKind {
    /// The path crossed a `.zip` boundary into a `cmdr_archive::ArchiveVolume`.
    Archive,
    /// The path reached into a repo's virtual `.git` trees, so a
    /// `cmdr_git::GitPortalVolume` serves it.
    GitPortal,
}

/// Outcome of a path-aware volume lookup: the volume that should serve `path`.
///
/// `path` is ALWAYS the caller's input path unchanged. A routed resolve only
/// swaps in the read-only volume — which maps the full path into its own
/// namespace itself — and a passthrough returns the requested volume untouched.
/// Adoption sites read `resolved.path` so the "full path, unchanged" contract
/// lives in exactly one place.
pub struct ResolvedVolume {
    /// The volume to call, or `None` when `volume_id` isn't registered (an
    /// unmount race). Sites keep their existing `.ok_or_else(...)?` handling.
    pub volume: Option<Arc<dyn Volume>>,
    /// The path to pass to `volume`'s methods — the input path, verbatim.
    pub path: PathBuf,
    /// Which route swapped `volume` in, or `None` for a passthrough. Sites use
    /// it to skip drive-index enrich/verify and the read-only write guards.
    pub routed: Option<RoutedKind>,
}

impl ResolvedVolume {
    /// An unrouted resolve: the requested volume (if any), path unchanged.
    pub fn passthrough(volume: Option<Arc<dyn Volume>>, path: &Path) -> Self {
        Self {
            volume,
            path: path.to_path_buf(),
            routed: None,
        }
    }

    /// Whether a route swapped the volume in. True for every read-only routed
    /// volume, whichever kind: a routed path has no drive-index entry and takes
    /// no writes, and both of those follow from being routed at all.
    pub fn is_routed(&self) -> bool {
        self.routed.is_some()
    }
}
