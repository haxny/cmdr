//! Real file system provider implementation.

use super::provider::FileSystemProvider;
use super::volume::backends::list_directory;
use cmdr_fs::entry::FileEntry;
use std::path::Path;

/// Real file system provider that accesses the actual file system.
pub struct RealFileSystemProvider;

impl FileSystemProvider for RealFileSystemProvider {
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
        list_directory(path, &crate::listing_host::backends::AppListings)
    }
}
