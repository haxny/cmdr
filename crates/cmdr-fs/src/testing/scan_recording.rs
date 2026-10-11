//! A [`ScanRecorder`] that writes down what it was told, for the cells that pin
//! which entries a backend's copy scan reports and in which path form.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;
use crate::volume::{CopyScanResult, ScanRecorder};

/// One call a scan made on its recorder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Recorded {
    /// A directory, about to be listed.
    Dir(PathBuf),
    /// A file and its size.
    File(PathBuf, u64),
    /// A subtree counted without a walk: `(path, files, bytes, is_directory)`.
    Subtree(PathBuf, usize, u64, bool),
}

/// Keeps every call, in arrival order.
// DEFAULT-OK: an empty recording is one nothing has been reported to yet.
#[derive(Default)]
pub struct ScanRecording {
    calls: Mutex<Vec<Recorded>>,
}

impl ScanRecording {
    /// Every call so far, sorted, so a cell compares sets regardless of the
    /// order a backend's listing came back in.
    pub fn sorted(&self) -> Vec<Recorded> {
        let mut calls = self.calls.lock_ignore_poison().clone();
        calls.sort();
        calls
    }
}

impl ScanRecorder for ScanRecording {
    fn file(&self, path: &Path, size: u64) {
        self.calls
            .lock_ignore_poison()
            .push(Recorded::File(path.to_path_buf(), size));
    }

    fn dir(&self, path: &Path) {
        self.calls.lock_ignore_poison().push(Recorded::Dir(path.to_path_buf()));
    }

    fn subtree(&self, path: &Path, scan: &CopyScanResult) {
        self.calls.lock_ignore_poison().push(Recorded::Subtree(
            path.to_path_buf(),
            scan.file_count,
            scan.total_bytes,
            scan.top_level_is_directory,
        ));
    }
}
