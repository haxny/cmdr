//! Gatekeeper's quarantine on everything an extract creates.
//!
//! **Why this exists.** A browser marks what it downloads with
//! `com.apple.quarantine`, and Gatekeeper checks a quarantined app before its
//! first launch. `ditto -x`, bsdtar, and Archive Utility copy an archive's
//! quarantine onto every file AND folder they extract (verified for the first two,
//! `docs/notes/transfer-metadata-experiments-2026-10-11.md` § "The quirks"), so an
//! app that came out of a downloaded zip is still checked. An extract that drops
//! it hands the user an app Gatekeeper never looks at: a security gap, not only
//! lost metadata.
//!
//! **What travels.** The archive file's own value, byte for byte, onto each item
//! the operation CREATED: every file it writes (an Overwrite replaces the file, so
//! the new one wears it too) and every folder its walk made, ❌ never a folder it
//! merged into. Only from an archive on a LOCAL parent (a remote archive has no
//! xattr to read, and Finder doesn't propagate one either), and only onto a local
//! destination. macOS only: Linux has no quarantine.
//!
//! **Where it happens.** A file gets it on the staged temp, after its last byte and
//! before the landing rename, and before `landed_mode`'s `chmod` (a path xattr
//! write on a `0444` file is `EACCES`, verified). A folder gets it the moment the
//! walk creates it, ❌ never in `folder_dates.rs`'s pass, which skips a folder with
//! no source date (a zip's implicit folders) and every folder of a cancelled or
//! failed subtree: exactly the shape that leaves an extracted `.app` bundle
//! unquarantined around quarantined files. An xattr on a fresh, empty folder
//! blocks nothing and bumps no date.
//!
//! **Cost.** One `getxattr` on the archive per operation (cached on
//! `WriteOperationState`), then one `setxattr` per created item, and only when the
//! archive had a quarantine. Every non-archive source answers `None` from
//! `Volume::local_backing_file` with no syscall. Best effort: a refused write is
//! logged at debug and never fails the copy.

use std::path::Path;
use std::sync::Arc;

use super::super::super::state::WriteOperationState;
use crate::file_system::volume::Volume;

/// Puts the archive's quarantine on `created`, an item this operation just made on
/// `dest_volume`: a file's staged temp before its landing rename, or a folder the
/// walk created. A no-op unless the source is a quarantined archive on a local
/// parent and the destination is a local filesystem.
#[cfg(target_os = "macos")]
pub(super) fn quarantine_created_item(
    state: &WriteOperationState,
    source_volume: &Arc<dyn Volume>,
    dest_volume: &Arc<dyn Volume>,
    created: &Path,
) {
    let Some(root) = dest_volume.local_path() else {
        return;
    };
    let Some(value) = archive_quarantine(state, source_volume) else {
        return;
    };
    // The same anchoring `LocalPosixVolume::resolve` applies, so this is the path
    // the destination actually wrote. One local syscall on a tokio worker that has
    // just written a whole file or made a folder: cheaper than a `spawn_blocking`
    // hop, and never the main thread.
    let absolute = cmdr_fs::volume::root_anchored(&root, created);
    if let Err(e) = xattr::set(&absolute, QUARANTINE_XATTR, &value) {
        log::debug!(
            target: "copy",
            "extract quarantine: couldn't set it on {} ({e}); the bytes are there either way",
            absolute.display(),
        );
    }
}

/// Linux has no quarantine to carry.
#[cfg(not(target_os = "macos"))]
pub(super) fn quarantine_created_item(
    _state: &WriteOperationState,
    _source_volume: &Arc<dyn Volume>,
    _dest_volume: &Arc<dyn Volume>,
    _created: &Path,
) {
}

#[cfg(target_os = "macos")]
const QUARANTINE_XATTR: &str = "com.apple.quarantine";

/// The quarantine of the local archive `source_volume` reads from, or `None` when
/// it isn't an archive, sits on a remote parent, or carries none.
///
/// Read once per operation and cached on `state`, keyed by the archive's path, so
/// an operation that somehow read from a second archive asks that one itself
/// instead of inheriting the first one's answer.
#[cfg(target_os = "macos")]
fn archive_quarantine<'a>(
    state: &'a WriteOperationState,
    source_volume: &Arc<dyn Volume>,
) -> Option<std::borrow::Cow<'a, [u8]>> {
    use std::borrow::Cow;

    let archive = source_volume.local_backing_file()?;
    let (read_for, value) = state
        .extract_quarantine
        .get_or_init(|| (archive.to_path_buf(), read_quarantine(archive)));
    if read_for == archive {
        value.as_deref().map(Cow::Borrowed)
    } else {
        read_quarantine(archive).map(Cow::Owned)
    }
}

#[cfg(target_os = "macos")]
fn read_quarantine(archive: &Path) -> Option<Vec<u8>> {
    match xattr::get(archive, QUARANTINE_XATTR) {
        Ok(value) => value,
        Err(e) => {
            log::debug!(
                target: "copy",
                "extract quarantine: couldn't read it from {} ({e}); extracting without one",
                archive.display(),
            );
            None
        }
    }
}
