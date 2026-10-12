//! Recording fakes for the seams, for tests that drive the engine, and test
//! registration on the app's shared engine.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::events::StalledOn;
use super::events::{ListingEnd, ListingEvents};
use super::{ListingObserver, ListingOverlay};
use crate::file_system::listing::diff::DirectoryDiffBatch;
use crate::file_system::listing::listing_id::ListingId;

/// Adds `overlay` to the shared engine. Test registration stays global until
/// each test builds its own engine; the first registration per id wins.
// shim: removed in M4b
pub(crate) fn register_overlay(overlay: Arc<dyn ListingOverlay>) {
    crate::file_system::listing::installed().add_overlay(overlay);
}

/// Adds `observer` to the shared engine, after every observer already there.
// shim: removed in M4b
pub(crate) fn register_observer(observer: Arc<dyn ListingObserver>) {
    crate::file_system::listing::installed().add_observer(observer);
}

/// Records every report, per kind and in one combined order.
// DEFAULT-OK: a recorder that has heard nothing yet holds empty lists.
#[derive(Default)]
pub(crate) struct RecordingEvents {
    pub opening: Mutex<Vec<String>>,
    pub stalled: Mutex<Vec<(String, StalledOn)>>,
    pub progress: Mutex<Vec<(String, usize)>>,
    pub read_complete: Mutex<Vec<(String, usize)>>,
    /// Per completed listing, its row count.
    pub complete: Mutex<Vec<(String, usize)>>,
    /// Per completed listing, the stored spelling it reported (`None` = as asked).
    pub stored_paths: Mutex<Vec<(String, Option<String>)>>,
    /// Per completed listing, whether it came back empty at its volume's root.
    pub empty_at_volume_root: Mutex<Vec<(String, bool)>>,
    pub cancelled: Mutex<Vec<String>>,
    /// Per failed listing, the error's message.
    pub failed: Mutex<Vec<(String, String)>>,
    pub diffs: Mutex<Vec<(String, Vec<DirectoryDiffBatch>)>>,
    pub respelled: Mutex<Vec<(String, String)>>,
    /// Every report's kind (`"opening"`, `"progress"`, `"complete"`, …), in order.
    pub sequence: Mutex<Vec<&'static str>>,
}

impl RecordingEvents {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The kinds of every report so far, in order.
    pub(crate) fn sequence(&self) -> Vec<&'static str> {
        self.sequence.lock_ignore_poison().clone()
    }

    fn note(&self, kind: &'static str) {
        self.sequence.lock_ignore_poison().push(kind);
    }
}

impl ListingEvents for RecordingEvents {
    fn opening(&self, id: &ListingId) {
        self.note("opening");
        self.opening.lock_ignore_poison().push(id.to_string());
    }

    fn stalled(&self, id: &ListingId, on: StalledOn) {
        self.note("stalled");
        self.stalled.lock_ignore_poison().push((id.to_string(), on));
    }

    fn progress(&self, id: &ListingId, loaded: usize) {
        self.note("progress");
        self.progress.lock_ignore_poison().push((id.to_string(), loaded));
    }

    fn read_complete(&self, id: &ListingId, total: usize) {
        self.note("read_complete");
        self.read_complete.lock_ignore_poison().push((id.to_string(), total));
    }

    fn finished(&self, id: &ListingId, end: ListingEnd) {
        let id = id.to_string();
        match end {
            ListingEnd::Complete {
                total_count,
                stored_path,
                empty_at_volume_root,
                ..
            } => {
                self.note("complete");
                self.complete.lock_ignore_poison().push((id.clone(), total_count));
                self.stored_paths.lock_ignore_poison().push((id.clone(), stored_path));
                self.empty_at_volume_root
                    .lock_ignore_poison()
                    .push((id, empty_at_volume_root));
            }
            ListingEnd::Cancelled { .. } => {
                self.note("cancelled");
                self.cancelled.lock_ignore_poison().push(id);
            }
            ListingEnd::Failed { error, .. } => {
                self.note("failed");
                self.failed.lock_ignore_poison().push((id, error.to_string()));
            }
        }
    }

    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>) {
        self.note("diff");
        self.diffs.lock_ignore_poison().push((id.to_string(), batches));
    }

    fn respelled(&self, id: &ListingId, path: &Path) {
        self.note("respelled");
        self.respelled
            .lock_ignore_poison()
            .push((id.to_string(), path.to_string_lossy().into_owned()));
    }
}
