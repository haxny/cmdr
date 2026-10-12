//! The kernel's mount table, answering what a stalled folder is mounted on.

use std::path::Path;

use crate::file_system::listing::host::{MountFacts, MountTable};

/// Reads `getfsstat` on macOS and `/proc/mounts` on Linux, lexically, by path
/// prefix. ❌ Never a `statfs` on the path, which blocks on the very mount that
/// just stalled.
pub(crate) struct AppMountTable;

impl MountTable for AppMountTable {
    #[cfg(target_os = "macos")]
    fn mount_of(&self, path: &Path) -> Option<MountFacts> {
        crate::volumes::mount_type_and_source_for(path).map(|(fs_type, source)| MountFacts { fs_type, source })
    }

    #[cfg(target_os = "linux")]
    fn mount_of(&self, path: &Path) -> Option<MountFacts> {
        crate::file_system::linux_mounts::mount_entry_for_path(path).map(|entry| MountFacts {
            fs_type: entry.fstype,
            source: entry.device,
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn mount_of(&self, _path: &Path) -> Option<MountFacts> {
        None
    }
}
