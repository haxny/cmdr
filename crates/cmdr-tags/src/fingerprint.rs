//! What identifies a file's state on disk, so a writer can tell "the file I read or wrote" from
//! "something else at that path now". The app checks it before each write; the undo store keeps
//! the one each apply left, and a restore leaves a file whose fingerprint moved alone.

use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// A file's size, modification time, and (on unix) inode and status-change time. A rewrite, a
/// replace, and a rename-over each move at least one; the ctime also catches a write that put
/// the mtime back (`touch -r`, a copy keeping dates). Serialized into undo records, so it
/// survives an app restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    size: u64,
    modified: Option<SystemTime>,
    inode: Option<u64>,
    changed: Option<(i64, i64)>,
}

impl Fingerprint {
    /// The file's fingerprint now; `None` when it can't be stat'ed. Follows a symlink, as a
    /// tag write does. Blocking: ❌ not under a lock.
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let (inode, changed) = unix_ids(&meta);
        Some(Self {
            size: meta.len(),
            modified: meta.modified().ok(),
            inode,
            changed,
        })
    }

    /// Whether the file at `path` still has `recorded`, a fingerprint taken earlier. No
    /// recorded fingerprint counts as changed. Blocking: ❌ not under a lock.
    pub fn holds(recorded: Option<&Self>, path: &Path) -> bool {
        recorded.is_some_and(|recorded| Self::of(path).as_ref() == Some(recorded))
    }
}

/// The inode and the status-change time (seconds, nanoseconds).
#[cfg(unix)]
fn unix_ids(meta: &std::fs::Metadata) -> (Option<u64>, Option<(i64, i64)>) {
    use std::os::unix::fs::MetadataExt;
    (Some(meta.ino()), Some((meta.ctime(), meta.ctime_nsec())))
}

#[cfg(not(unix))]
fn unix_ids(_meta: &std::fs::Metadata) -> (Option<u64>, Option<(i64, i64)>) {
    (None, None)
}
