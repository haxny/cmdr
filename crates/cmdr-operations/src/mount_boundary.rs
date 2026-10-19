//! Where a delete's walk must stop: a directory another filesystem is mounted on.
//!
//! A recursive delete that walks into a mount empties that volume, often a NAS
//! or a server other people use, and the mount point's own `rmdir` then fails
//! quietly, after its contents are gone. Both delete walkers ask this of every
//! directory they're about to descend and refuse the whole delete on a yes,
//! before anything is deleted, which is what Finder does.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use super::state::WriteOperationState;
use super::types::WriteOperationError;

/// Whether the directory at `path` is where another filesystem is mounted.
/// `dev` is the device its own `lstat` reported.
///
/// A device that differs from the parent's is the cheap first test, so an
/// ordinary directory costs one `stat` of its parent and nothing else. The OS
/// mount table settles the rest: a macOS firmlink (`/Users` under `/`) crosses
/// from the system volume to the data volume with no mount at that path. A
/// table that can't be read answers "boundary", since the caller is about to
/// delete on the answer.
///
/// ❗ Blocks on a `stat`, so an async caller runs it on the blocking pool.
pub(crate) fn is_mount_boundary(path: &Path, dev: u64) -> bool {
    use std::os::unix::fs::MetadataExt;

    // `/` has no parent to cross from.
    let Some(parent) = path.parent() else {
        return false;
    };
    // Following: the parent may be spelled through a link (`/tmp`), and what
    // holds `path` is the link's target.
    let Ok(parent_meta) = std::fs::metadata(parent) else {
        return false;
    };
    if parent_meta.dev() == dev {
        return false;
    }
    // The table spells a mount point with its links resolved (`/private/tmp/…`).
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    super::transfer_sides::root_is_listed(&resolved).unwrap_or(true)
}

/// Refuses the delete when `path` is a directory another filesystem is mounted
/// on, for a volume backed by a local folder (`Volume::local_path`): a drive or
/// share mounted inside a `LocalPosixVolume` pane's tree is reachable through
/// it. Other backends have no local path to ask about; what's mounted on a
/// server is the server's.
pub(crate) async fn refuse_a_volume_mount_point(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    use std::os::unix::fs::MetadataExt;

    let Some(root) = volume.local_path() else {
        return Ok(());
    };
    let on_disk = cmdr_fs::volume::root_anchored(&root, path);
    let mounted = state
        .scope()
        .host()
        .runtime()
        .spawn_blocking(move || {
            // A path that can't be stat'd isn't refused here: the listing that
            // follows reports it.
            let dev = std::fs::symlink_metadata(&on_disk).ok()?.dev();
            is_mount_boundary(&on_disk, dev).then_some(on_disk)
        })
        .await
        .map_err(|e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: format!("the mount check didn't finish: {e}"),
        })?;
    match mounted {
        Some(mount_point) => Err(WriteOperationError::MountedVolumeInside {
            mount_point: mount_point.display().to_string(),
        }),
        None => Ok(()),
    }
}
