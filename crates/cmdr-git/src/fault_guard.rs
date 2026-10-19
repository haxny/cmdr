//! Throwing away an answer read through a mapped page that went away.
//!
//! `gix` memory-maps pack files, their indexes, and the commit-graph. When the disk under a repo
//! goes away mid-read, the app's SIGBUS handler maps zeros over the dead page instead of crashing
//! (`cmdr_fs::mapped_file_faults`), and the decoder that read them carries on with corrupt bytes.
//! [`FaultWatch`] is the other half: it reads the recovery's generation before an operation and
//! again after, and an operation that straddled a recovery answers
//! [`FriendlyGitErrorKind::VolumeGone`] whatever it computed.
//!
//! **Address-free on purpose.** The fault can land on a thread `gix` spawned itself (the status
//! walk's), so nothing thread-local would see it, and nothing maps a fault address back to a repo.
//! The cost is that an operation on a healthy repo that happened to run while ANOTHER repo's disk
//! went away is refused once too; the next call answers normally. `DETAILS.md` § "A disk that goes
//! away mid-read".
//!
//! [`GuardedRead`] is what an entry point takes: the watch plus a `ReadLease` (`releasing.rs`), so
//! a volume about to unmount can stop the read, which then answers `VolumeGone` too.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cmdr_fs::mapped_file_faults;
use cmdr_fs::volume::git_error::{FriendlyGitError, FriendlyGitErrorKind};

use crate::releasing::ReadLease;

/// The recovery generation an operation started at.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FaultWatch(u64);

impl FaultWatch {
    /// Starts watching. Take it BEFORE the first `gix` read.
    pub(crate) fn start() -> Self {
        Self(mapped_file_faults::generation())
    }

    /// Whether a mapped page was recovered since [`start`](Self::start).
    pub(crate) fn tripped(&self) -> bool {
        mapped_file_faults::generation() != self.0
    }

    /// `answer`, unless a mapped page was recovered while it was computed: then
    /// [`FriendlyGitErrorKind::VolumeGone`] about `root`, `answer` is dropped, and so is
    /// `root`'s cached status. The open handles go at the cache's next lookup
    /// (`RepoCache::discover`), which needs no root to know they're suspect.
    #[cfg(test)]
    pub(crate) fn vouch<T>(&self, root: &Path, answer: Result<T, FriendlyGitError>) -> Result<T, FriendlyGitError> {
        match self.refusal(root) {
            Some(refused) => Err(refused),
            None => answer,
        }
    }

    /// The `VolumeGone` an operation on `root` answers instead of its own answer, or `None`
    /// when no mapped page was recovered since [`start`](Self::start). Drops `root`'s cached
    /// status when it refuses.
    pub(crate) fn refusal(&self, root: &Path) -> Option<FriendlyGitError> {
        if !self.tripped() {
            return None;
        }
        crate::status::invalidate_status_cache(root);
        Some(volume_gone(root))
    }
}

/// What an operation on `root` answers when its volume went away, or is about to.
pub(crate) fn volume_gone(root: &Path) -> FriendlyGitError {
    FriendlyGitError::new(FriendlyGitErrorKind::VolumeGone, root.display().to_string())
}

/// One read through `gix` on the repo at a root: watched for a recovered page, and stoppable by a
/// volume about to unmount (`releasing.rs`). Either makes it answer `VolumeGone`.
pub(crate) struct GuardedRead {
    watch: FaultWatch,
    lease: ReadLease,
    root: PathBuf,
}

impl GuardedRead {
    /// Starts a read on the repo whose worktree root is `root`. Take it BEFORE the first `gix`
    /// read, and keep it until the answer is out.
    pub(crate) fn begin(root: &Path) -> Self {
        Self {
            watch: FaultWatch::start(),
            lease: ReadLease::take(root),
            root: root.to_path_buf(),
        }
    }

    /// The flag a long walk polls; set when the read's volume is about to unmount.
    pub(crate) fn stop_flag(&self) -> Arc<AtomicBool> {
        self.lease.stop_flag()
    }

    /// The `VolumeGone` this read answers instead of its own answer, or `None` when its answer
    /// can be trusted.
    pub(crate) fn refusal(&self) -> Option<FriendlyGitError> {
        if self.lease.stopped() {
            return Some(volume_gone(&self.root));
        }
        self.watch.refusal(&self.root)
    }

    /// `answer`, unless [`refusal`](Self::refusal) has one.
    pub(crate) fn vouch<T>(&self, answer: Result<T, FriendlyGitError>) -> Result<T, FriendlyGitError> {
        match self.refusal() {
            Some(refused) => Err(refused),
            None => answer,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cmdr_fs::mapped_file_faults::simulate_recovery;
    use cmdr_fs::volume::git_error::FriendlyGitErrorKind;

    use super::FaultWatch;

    #[test]
    fn an_answer_with_no_recovery_under_it_goes_through() {
        let watch = FaultWatch::start();
        assert_eq!(watch.vouch(Path::new("/repo"), Ok(7)).ok(), Some(7));
        assert!(watch.refusal(Path::new("/repo")).is_none());
    }

    #[test]
    fn an_answer_computed_across_a_recovery_is_refused_as_volume_gone() {
        let watch = FaultWatch::start();
        simulate_recovery();
        let refused = watch
            .vouch(Path::new("/Volumes/USB/repo"), Ok(7))
            .expect_err("zeros may be in it");
        assert_eq!(refused.kind, FriendlyGitErrorKind::VolumeGone);
        assert_eq!(refused.path, "/Volumes/USB/repo");
    }

    #[test]
    fn a_watch_started_after_the_recovery_vouches_again() {
        simulate_recovery();
        let watch = FaultWatch::start();
        assert_eq!(watch.vouch(Path::new("/repo"), Ok(7)).ok(), Some(7));
    }
}
