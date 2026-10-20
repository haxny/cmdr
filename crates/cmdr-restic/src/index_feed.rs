//! [`IndexFeed`]: what the storage adapter reads during an index load: the index
//! files the load read ahead of rustic, the load's cancel, and its byte tally.
//!
//! A leaf on purpose. The adapter (`backend.rs`) reads it and the load
//! (`index_load.rs`) fills it, so neither has to name the other's module.

use bytes::Bytes;
use cmdr_fs::volume::ListingPhase;
use rustic_core::Id;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// The storage adapter's side of a load: the index files read ahead of rustic,
/// the load's cancel, and its tally.
pub(crate) struct IndexFeed {
    prefetched: Mutex<HashMap<Id, Bytes>>,
    cancel: CancellationToken,
    tally: Tally,
}

impl IndexFeed {
    /// A feed for a load of `total_files` index files, `total_bytes` in all,
    /// that publishes each move of its tally to `progress` (and the starting
    /// zero right away).
    pub(crate) fn new(
        cancel: CancellationToken,
        total_bytes: u64,
        total_files: u32,
        progress: watch::Sender<Option<ListingPhase>>,
    ) -> Self {
        let tally = Tally {
            done_bytes: AtomicU64::new(0),
            done_files: AtomicU32::new(0),
            total_bytes,
            total_files,
            state: progress,
        };
        tally.publish();
        Self {
            prefetched: Mutex::new(HashMap::new()),
            cancel,
            tally,
        }
    }

    /// The bytes of index file `id` when the prefetch read them. Handed over,
    /// not copied, so the memory goes as rustic takes each one.
    pub(crate) fn take(&self, id: &Id) -> Option<Bytes> {
        lock(&self.prefetched).remove(id)
    }

    /// Files the bytes the prefetch read for index file `id`.
    pub(crate) fn put(&self, id: Id, bytes: Bytes) {
        lock(&self.prefetched).insert(id, bytes);
    }

    /// Lets go of anything rustic didn't ask for (a file gone between the two
    /// listings).
    pub(crate) fn clear(&self) {
        lock(&self.prefetched).clear();
    }

    /// The load's cancel: fired when every waiter left.
    pub(crate) fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Whether every waiter left: the adapter refuses new index reads then.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// `bytes` more of an index file arrived.
    pub(crate) fn add_bytes(&self, bytes: usize) {
        self.tally.add_bytes(bytes);
    }

    /// One more index file arrived whole.
    pub(crate) fn file_done(&self) {
        self.tally.file_done();
    }

    /// rustic read an index file itself (not prefetched), `bytes` long.
    pub(crate) fn note_read(&self, bytes: usize) {
        self.tally.add_bytes(bytes);
        self.tally.file_done();
    }
}

/// The index files' running tally, published to the load's waiters as it moves.
struct Tally {
    done_bytes: AtomicU64,
    done_files: AtomicU32,
    total_bytes: u64,
    total_files: u32,
    state: watch::Sender<Option<ListingPhase>>,
}

impl Tally {
    fn add_bytes(&self, bytes: usize) {
        self.done_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
        self.publish();
    }

    fn file_done(&self) {
        self.done_files.fetch_add(1, Ordering::Relaxed);
        self.publish();
    }

    fn publish(&self) {
        let phase = ListingPhase::LoadingIndex {
            done_bytes: self.done_bytes.load(Ordering::Relaxed),
            total_bytes: self.total_bytes,
            done_files: self.done_files.load(Ordering::Relaxed),
            total_files: self.total_files,
        };
        self.state.send_replace(Some(phase));
    }
}

/// Locks a std mutex, taking the data over from a thread that panicked holding
/// it: the map stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
