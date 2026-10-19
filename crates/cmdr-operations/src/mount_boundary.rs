//! Where a tree removal must stop: a directory another filesystem is mounted on.
//!
//! A recursive delete that walks into a mount empties that volume, often a NAS
//! or a server other people use, and the mount point's own `rmdir` then fails
//! quietly, after its contents are gone. Both delete walkers ask
//! [`is_mount_boundary`] of every directory they're about to descend, and an
//! Overwrite that replaces a folder asks [`refuse_a_mount_inside`] before it
//! sets the folder aside; either refuses the whole operation on a yes, before
//! anything is removed, which is what Finder does.

use std::path::{Path, PathBuf};
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

/// The first mount point at or under the directory `dir`, spelled under `dir`,
/// read from the OS mount table: an Overwrite is about to remove the whole tree,
/// and the table answers without walking it. `None` for a `dir` that isn't a
/// directory (or isn't there). A table that can't be read answers `dir` itself,
/// since the caller is about to remove on the answer.
///
/// ❗ Blocks on a `stat`, so an async caller runs it on the blocking pool.
pub(crate) fn mount_at_or_under(dir: &Path) -> Option<PathBuf> {
    if !std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir()) {
        return None;
    }
    // The table spells mount points with links resolved.
    let resolved = dir.canonicalize().ok()?;
    let Some(mount_points) = mount_points() else {
        return Some(dir.to_path_buf());
    };
    mount_points
        .into_iter()
        .find_map(|mount| mount.strip_prefix(&resolved).ok().map(|below| dir.join(below)))
}

/// Every mount point in the OS mount table, or `None` when it can't be read.
fn mount_points() -> Option<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        cmdr_local::mount_table::enumerate_mounts()
            .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mount_point)).collect())
    }
    #[cfg(target_os = "linux")]
    {
        cmdr_local::linux_mounts::parse_proc_mounts()
            .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mountpoint)).collect())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Refuses an Overwrite that would remove the local folder `dest` while a drive
/// is mounted at or under it.
pub(crate) fn refuse_a_mount_inside(dest: &Path) -> Result<(), WriteOperationError> {
    match mount_at_or_under(dest) {
        Some(mount_point) => Err(refused_at(&mount_point)),
        None => Ok(()),
    }
}

/// [`is_mount_boundary`] for a directory the volume walker is about to descend.
pub(crate) async fn refuse_a_volume_mount_point(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    on_local_disk(volume, path, state, |on_disk| {
        use std::os::unix::fs::MetadataExt;
        // A path that can't be stat'd isn't refused here: the listing that
        // follows reports it.
        let dev = std::fs::symlink_metadata(&on_disk).ok()?.dev();
        is_mount_boundary(&on_disk, dev).then_some(on_disk)
    })
    .await
}

/// [`refuse_a_mount_inside`] for a volume folder an Overwrite is about to set
/// aside and remove.
pub(crate) async fn refuse_a_volume_mount_inside(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    on_local_disk(volume, path, state, |on_disk| mount_at_or_under(&on_disk)).await
}

/// Runs `find_mount` on the blocking pool over `path`'s spot on disk, for a
/// volume backed by a local folder (`Volume::local_path`): a drive or share
/// mounted inside a `LocalPosixVolume` pane's tree is reachable through it.
/// Other backends have no local path to ask about; what's mounted on a server
/// is the server's.
async fn on_local_disk(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
    find_mount: fn(PathBuf) -> Option<PathBuf>,
) -> Result<(), WriteOperationError> {
    let Some(root) = volume.local_path() else {
        return Ok(());
    };
    let on_disk = cmdr_fs::volume::root_anchored(&root, path);
    let mounted = state
        .scope()
        .host()
        .runtime()
        .spawn_blocking(move || find_mount(on_disk))
        .await
        .map_err(|e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: format!("the mount check didn't finish: {e}"),
        })?;
    match mounted {
        Some(mount_point) => Err(refused_at(&mount_point)),
        None => Ok(()),
    }
}

fn refused_at(mount_point: &Path) -> WriteOperationError {
    WriteOperationError::MountedVolumeInside {
        mount_point: mount_point.display().to_string(),
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "mount_boundary_test_support.rs"]
pub(crate) mod test_support;
