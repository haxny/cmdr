//! What a filesystem path is mounted on, for a stalled listing's wording.

use std::path::Path;

/// One mount-table row, as the kernel lists it.
pub struct MountFacts {
    /// The filesystem type (`"smbfs"`, `"apfs"`, `"nfs4"`).
    pub fs_type: String,
    /// What's mounted (`"//user@nas/share"`, `"/dev/disk3s1"`).
    pub source: String,
}

/// Reads the kernel's mount table.
///
/// ❗ A lookup by path prefix only: ❌ never a `statfs` on the path, which blocks
/// on the very mount that just stalled. The engine calls it lexically first and
/// only resolves symlinks, off-thread and bounded, when the answer was a local
/// disk.
pub trait MountTable: Send + Sync {
    /// The mount `path` lives on, or `None` when the table can't say.
    fn mount_of(&self, path: &Path) -> Option<MountFacts>;
}

/// No mount table: every folder's mount is unknown.
pub struct NoMountTable;

impl MountTable for NoMountTable {
    fn mount_of(&self, _path: &Path) -> Option<MountFacts> {
        None
    }
}
