//! Coalesces transport, never coordinate spaces or committed revisions.
//! Producers stamp and enqueue each transition under the listing-cache write lock.
//! Lock order is cache then queue; draining the queue never reads the cache.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Handle;

use crate::file_system::listing::diff::DirectoryDiffBatch;
use crate::file_system::listing::host::ListingEvents;
use crate::file_system::listing::listing_id::ListingId;
use cmdr_fs::ignore_poison::IgnorePoison;

// shim: removed in M4b. Callers outside the engine name these here; they forward to the installed engine.
#[cfg(any(test, feature = "playwright-e2e"))]
pub(crate) use crate::file_system::listing::shims::diff_emitter::*;

// shim: removed in M4b. Callers outside the engine name these here; they forward to the installed engine.

const FLUSH_WINDOW_MS: u64 = 50;

// DEFAULT-OK: an empty queue has no flush scheduled.
#[derive(Default)]
struct PendingDiff {
    batches: Vec<DirectoryDiffBatch>,
    flush_scheduled: bool,
}

/// One engine's committed diff batches, per listing, waiting for their flush.
pub(crate) struct DiffQueue {
    pending: Mutex<HashMap<String, PendingDiff>>,
    /// Where a flush reports.
    events: Arc<dyn ListingEvents>,
    /// Where the flush timer runs.
    runtime: Handle,
}

impl DiffQueue {
    pub(crate) fn new(events: Arc<dyn ListingEvents>, runtime: Handle) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            events,
            runtime,
        }
    }

    /// Takes an already stamped transition while its producer holds the cache write lock.
    pub(crate) fn enqueue(self: &Arc<Self>, listing_id: &str, batch: DirectoryDiffBatch) {
        // Projection changes can advance a revision with no rows affected (a scratch
        // dotfile while hidden files are off). Clients still need that revision link.
        let needs_schedule = {
            let mut pending = self.pending.lock_ignore_poison();
            let entry = pending.entry(listing_id.to_string()).or_default();
            entry.batches.push(batch);
            if entry.flush_scheduled {
                false
            } else {
                entry.flush_scheduled = true;
                true
            }
        };
        if needs_schedule {
            let lid = listing_id.to_string();
            // A strong handle: the task ends after one flush window.
            let queue = Arc::clone(self);
            self.runtime.spawn(async move {
                tokio::time::sleep(Duration::from_millis(FLUSH_WINDOW_MS)).await;
                queue.flush(&lid);
            });
        }
    }

    /// Call under the cache write lock when replacing a row space or ending a listing.
    pub(crate) fn drop_pending(&self, listing_id: &str) {
        self.pending.lock_ignore_poison().remove(listing_id);
    }

    pub(crate) fn take_pending(&self, listing_id: &str) -> Vec<DirectoryDiffBatch> {
        let mut pending = self.pending.lock_ignore_poison();
        let Some(entry) = pending.get_mut(listing_id) else {
            return Vec::new();
        };
        entry.flush_scheduled = false;
        std::mem::take(&mut entry.batches)
    }

    pub(crate) fn flush(&self, listing_id: &str) {
        let batches = self.take_pending(listing_id);
        if batches.is_empty() {
            return;
        }
        self.events.diff(&ListingId::new(listing_id), batches);
    }

    #[cfg(feature = "playwright-e2e")]
    pub(crate) fn flush_all_pending(&self) {
        let ids: Vec<String> = self.pending.lock_ignore_poison().keys().cloned().collect();
        for id in ids {
            self.flush(&id);
        }
    }

    /// Marks `listing_id`'s flush as already scheduled, so a test reads its
    /// queue without racing the flush timer.
    #[cfg(test)]
    pub(crate) fn hold(&self, listing_id: &str) {
        self.pending
            .lock_ignore_poison()
            .entry(listing_id.to_string())
            .or_default()
            .flush_scheduled = true;
    }

    #[cfg(test)]
    pub(crate) fn pending_batches(&self, listing_id: &str) -> Vec<DirectoryDiffBatch> {
        self.pending
            .lock_ignore_poison()
            .get(listing_id)
            .map(|e| e.batches.clone())
            .unwrap_or_default()
    }
}
