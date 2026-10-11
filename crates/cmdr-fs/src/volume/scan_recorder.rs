//! What a copy scan saw, entry by entry, for a caller that needs the tree's
//! SHAPE and not only its totals.
//!
//! A transfer shows the selection as a live tree (which folder is in flight,
//! what landed, what's next), and the only walk that ever sees every entry of a
//! remote selection is the copy scan. So the scan hands each entry to an
//! optional recorder riding on the [`ScanBoundary`](super::ScanBoundary), the
//! one seam every walk already calls per entry: a backend can't count an entry
//! without recording it, and nobody pays for it when nobody listens.
//!
//! ❗ **Paths are in the CALLER's form**: the path the batch scan was handed for a
//! source, joined with each child's name below it. A backend that walks in its
//! own path space (SMB's share-relative paths, MTP's device paths) carries the
//! caller's path down beside its own; a recorded path the caller can't place
//! under one of its sources is a path it has to drop.

use std::path::Path;

use super::CopyScanResult;

/// Receives every entry a copy scan walks.
///
/// Called from the scan's own task, possibly from several tasks of one scan in
/// turn, so an implementation guards its state with a lock. Each call is cheap
/// bookkeeping (an arena push); ❌ never I/O, which would stall the walk.
pub trait ScanRecorder: Send + Sync {
    /// A file at `path`, `size` bytes. A symlink, which a scan never follows,
    /// arrives here too: it copies as one entry.
    fn file(&self, path: &Path, size: u64);

    /// A directory at `path`, reported before its listing goes out. A selected
    /// directory arrives here as well as every directory below it.
    fn dir(&self, path: &Path);

    /// A whole subtree at `path` that was counted without being walked here (a
    /// backend's own per-path scan, a cached total): its totals are known, its
    /// children aren't. `scan.top_level_is_directory` says whether `path` is a
    /// folder at all.
    fn subtree(&self, path: &Path, scan: &CopyScanResult);
}
