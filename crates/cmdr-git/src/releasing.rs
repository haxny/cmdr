//! The reads in flight on each repo, so a volume about to unmount can stop the ones on it.
//!
//! A plain unmount fails while any file on the volume is mapped, and a `gix` read keeps its
//! repo's packs mapped until it's done (verified with a RAM disk on Darwin 27.0.0 arm64,
//! 2026-10-19: `diskutil unmount` refuses while one mapping is open). So letting go of a volume
//! (`GitPortal::release_volume`) has to stop the long reads on it, the status walk and the commit
//! walk, rather than wait them out. Each takes a [`ReadLease`] for its repo's root; the lease's
//! stop flag is what `gix` and the commit walk poll.
//!
//! **Why a static, when everything mutable here is a field on `GitPortal`.** The status walk is
//! reachable from `list_status` and `repo_info` with a repo handle alone, no portal, so the one
//! place both a walk and `release_volume` can meet is this registry. Each entry lives exactly as
//! long as its read, and nothing here is an answer anyone caches.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

struct InFlight {
    id: u64,
    root: PathBuf,
    stop: Arc<AtomicBool>,
}

static READS: Mutex<Vec<InFlight>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// One read in flight on the repo at `root`, registered until dropped.
pub(crate) struct ReadLease {
    id: u64,
    stop: Arc<AtomicBool>,
}

impl ReadLease {
    /// Registers a read on the repo whose worktree root is `root`.
    pub(crate) fn take(root: &Path) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        READS.lock_ignore_poison().push(InFlight {
            id,
            root: root.to_path_buf(),
            stop: Arc::clone(&stop),
        });
        Self { id, stop }
    }

    /// The flag a walk polls: set when the read's volume is about to unmount.
    pub(crate) fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    /// Whether the read's volume asked it to stop.
    pub(crate) fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

impl Drop for ReadLease {
    fn drop(&mut self) {
        READS.lock_ignore_poison().retain(|read| read.id != self.id);
    }
}

/// Tells every read on a repo under `prefix` to stop.
pub(crate) fn stop_reads_under(prefix: &Path) {
    for read in READS
        .lock_ignore_poison()
        .iter()
        .filter(|read| read.root.starts_with(prefix))
    {
        read.stop.store(true, Ordering::SeqCst);
    }
}

/// How many reads on a repo under `prefix` are still in flight.
pub(crate) fn reads_under(prefix: &Path) -> usize {
    READS
        .lock_ignore_poison()
        .iter()
        .filter(|read| read.root.starts_with(prefix))
        .count()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ReadLease, reads_under, stop_reads_under};

    #[test]
    fn a_volume_stops_only_the_reads_on_it_and_counts_them_until_they_finish() {
        let on_it = ReadLease::take(Path::new("/Volumes/releasing-cell-usb/repo"));
        let elsewhere = ReadLease::take(Path::new("/Users/releasing-cell/repo"));
        let prefix = Path::new("/Volumes/releasing-cell-usb");
        assert_eq!(reads_under(prefix), 1);

        stop_reads_under(prefix);

        assert!(on_it.stopped());
        assert!(!elsewhere.stopped(), "a read on another disk keeps going");
        drop(on_it);
        assert_eq!(reads_under(prefix), 0, "a finished read is no longer waited for");
    }

    #[test]
    fn a_prefix_matches_whole_path_segments_only() {
        let lookalike = ReadLease::take(Path::new("/Volumes/releasing-cell-usb2/repo"));
        stop_reads_under(Path::new("/Volumes/releasing-cell-usb"));
        assert!(!lookalike.stopped(), "`usb` isn't `usb2`");
    }
}
