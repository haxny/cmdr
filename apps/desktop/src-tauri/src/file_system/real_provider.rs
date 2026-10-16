//! Real file system provider implementation.

use super::provider::FileSystemProvider;
use cmdr_fs::entry::FileEntry;
use cmdr_local::reading::list_directory;
use std::path::Path;

/// Real file system provider that accesses the actual file system.
pub struct RealFileSystemProvider;

impl FileSystemProvider for RealFileSystemProvider {
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
        list_directory(path, &crate::listing_host::backends::AppListings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::fs;

    #[test]
    fn test_list_directory() {
        let provider = RealFileSystemProvider;
        // Create our own temp directory to avoid permission issues
        let temp_dir = TestDir::new("list_test");

        let result = provider.list_directory(&temp_dir);

        assert!(result.is_ok(), "list_directory failed: {:?}", result.err());
    }

    #[test]
    fn test_list_directory_entries_have_names() {
        let provider = RealFileSystemProvider;
        let temp_dir = TestDir::new("ops_test");

        let test_file = temp_dir.join("test_file.txt");
        fs::write(&test_file, "content").unwrap();

        let entries = provider.list_directory(&temp_dir).unwrap();

        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.name == "test_file.txt"));
    }
}
