//! Changing names on the export: folders, deletes, renames, and patching the
//! panes' listings afterwards.
//!
//! - ❗ Every mutation resolves its folder through
//!   [`NfsVolume::dir_for_mutation`] (the parent chain proven first) and goes
//!   by `dir handle + name`, ❌ never by a leaf's cached handle.
//! - ❗ After a success, hand the reply's `wcc_data` to
//!   `HandleCache::note_dir_changed`, so the next mutation in the same folder
//!   needn't re-LOOKUP.
//! - `rename(force = false)` claims the name first (`CREATE GUARDED` or
//!   `MKDIR`): v3 `RENAME` replaces. The plan's § "Rename without clobbering".

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{LeadsTo, MakesDirectories};
use cmdr_fs::volume::patching::PatchSource;
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, VolumeError};

use super::NfsVolume;
use super::handles::Follow;
use crate::proto::nfs3::FileType;

impl NfsVolume {
    /// Makes exactly one folder (`MKDIR`), refusing a taken name.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// `mkdir -p`, through the shared walk (`cmdr_fs::volume::mkdir_all`).
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Removes one node: `REMOVE`, or `RMDIR` for a folder. ❌ Never recursive.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Renames `from` to `to`; `force` false never replaces what's at `to`.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let _ = (from, to, force);
        Err(VolumeError::NotSupported)
    }
}

impl MakesDirectories for NfsVolume {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        self.to_remote_path(path)
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(async move {
            let _ = remote;
            Err(VolumeError::NotSupported)
        })
    }

    /// One walk that follows links, the `stat` the shared walk asks for.
    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo> {
        Box::pin(async move {
            let client = self.client().await?;
            match self.resolve(&client, remote, Follow::Always, true).await {
                Ok(found)
                    if found
                        .attributes
                        .is_none_or(|attrs| attrs.file_type == FileType::Directory) =>
                {
                    Ok(LeadsTo::Directory)
                }
                Ok(_) => Ok(LeadsTo::NotADirectory),
                Err(VolumeError::NotFound(_)) => Ok(LeadsTo::Nothing),
                Err(VolumeError::NotADirectory(_)) => Ok(LeadsTo::NotADirectory),
                Err(err) => Err(err),
            }
        })
    }
}

impl PatchSource for NfsVolume {
    fn patch_volume_id(&self) -> &str {
        self.volume_id()
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.inner.host.listings()
    }

    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        self.display_path_for(path)
    }
}
