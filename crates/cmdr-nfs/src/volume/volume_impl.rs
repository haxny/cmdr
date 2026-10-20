//! The `Volume` trait for [`NfsVolume`]: every method a delegator.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{BackendKind, ListingProgress, SignInShape, Volume, VolumeError};

use super::NfsVolume;

impl Volume for NfsVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        self.root.app_root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn backend_kind(&self) -> BackendKind {
        BackendKind::Nfs
    }

    /// ❗ `AUTH_SYS` has no secret, so there is never a sheet to show.
    fn sign_in_prompt(&self) -> SignInShape {
        SignInShape::Nothing
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
}
