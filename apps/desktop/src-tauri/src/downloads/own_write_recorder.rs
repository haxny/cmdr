//! Test-only recorder behind the Cmdr-own-write hook: every
//! [`super::note_pending_write_for_cmdr`] call, with whether its path existed
//! yet, so a test can pin what reaches the hook (the app's `OwnWrites` answer,
//! `operations_host/own_writes.rs`). The engine's own announce order is pinned
//! against its seam (`write_operations/own_write_announce_tests.rs`).
//!
//! One process-wide registry keyed by path prefix, never per thread: managed
//! ops and the volume engine's writers run on the blocking pool, so a
//! thread-local recorder on the test thread would record nothing and an
//! "every announce came first" assertion would pass on zero records. Each test
//! registers its own `TestDir` root and reads back only the paths under it, so
//! siblings can't fill each other's records.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::ignore_poison::IgnorePoison;

/// One hook call: the path as the write path passed it, and whether anything
/// (file, folder, or symlink) sat there at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Announced {
    pub(crate) path: PathBuf,
    pub(crate) existed: bool,
}

type Log = Arc<Mutex<Vec<Announced>>>;

static RECORDINGS: Mutex<Vec<(PathBuf, Log)>> = Mutex::new(Vec::new());

/// Records every announce under `root` until dropped.
pub(crate) struct OwnWriteRecording {
    log: Log,
}

/// Starts recording announces for paths under `root`.
pub(crate) fn record_under(root: &Path) -> OwnWriteRecording {
    let log: Log = Arc::default();
    RECORDINGS
        .lock_ignore_poison()
        .push((root.to_path_buf(), Arc::clone(&log)));
    OwnWriteRecording { log }
}

impl OwnWriteRecording {
    /// Every announce under the root so far, in call order.
    pub(crate) fn announced(&self) -> Vec<Announced> {
        self.log.lock_ignore_poison().clone()
    }
}

impl Drop for OwnWriteRecording {
    fn drop(&mut self) {
        RECORDINGS
            .lock_ignore_poison()
            .retain(|(_, log)| !Arc::ptr_eq(log, &self.log));
    }
}

/// Called from the hook for every announce.
pub(super) fn note(path: &Path) {
    let logs: Vec<Log> = RECORDINGS
        .lock_ignore_poison()
        .iter()
        .filter(|(root, _)| path.starts_with(root))
        .map(|(_, log)| Arc::clone(log))
        .collect();
    if logs.is_empty() {
        return;
    }
    let announced = Announced {
        path: path.to_path_buf(),
        existed: path.symlink_metadata().is_ok(),
    };
    for log in logs {
        log.lock_ignore_poison().push(announced.clone());
    }
}
