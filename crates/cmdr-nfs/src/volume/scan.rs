//! What this backend hands the shared tree walk: one walk for a stat, one
//! listing for a folder. The walk itself, the batch loop, and the conflict
//! matcher: `cmdr_fs::volume::scan_walk`.
//!
//! ❗ Both are the backend's OWN read path, ❌ never a listing-cache lookup:
//! with no watcher, `listing_watch_coverage` is `None` and a cached listing is
//! only as fresh as the last look.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::scan_walk::{ScanSource, Walking};

use super::NfsVolume;

impl ScanSource for NfsVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        Box::pin(self.list_directory_impl(path, None, None))
    }
}
