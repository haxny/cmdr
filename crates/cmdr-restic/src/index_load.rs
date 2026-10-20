//! Loading a repo's full index: one load per repo however many listings wait on
//! it, cancelled when the last of them leaves, with progress in real bytes.
//!
//! rustic's own load reads every index file in parallel on rayon's GLOBAL pool,
//! each read a pool worker blocked in our adapter for a whole NAS round trip
//! (Cmdr's search and icons share that pool), it can't be cancelled, and its
//! progress counts files. So the load reads the index files itself first, on the
//! async runtime, a few at a time, counting bytes and checking the cancel between
//! chunks; then rustic builds the index from those bytes in memory (the adapter
//! serves them through an [`IndexFeed`]). Above [`PREFETCH_LIMIT`] the bytes would
//! cost too much to hold, so rustic reads the files itself and the feed only
//! counts them and refuses new reads once cancelled.

use crate::backend::VolumeBackend;
use crate::index_feed::IndexFeed;
use crate::repo::{IndexedRepo, ResticError, open_blocking, worker_crashed};
use bytes::Bytes;
use cmdr_fs::volume::{ListingPhase, ListingProgress, Volume, VolumeError};
use rustic_core::{FileType, Id};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// How many index files the prefetch reads at once. rustic's own load keeps one
/// per core in flight (16 on an M3 Max); four keep a NAS busy without crowding
/// out the browsing the load is for.
const PREFETCH_CONCURRENCY: usize = 4;

/// The most index bytes the prefetch holds for the length of a load. 112 MB of
/// index files on a 2.4M-blob repo sit on top of a ~560 MB peak rustic reaches
/// anyway (`docs/notes/restic/open-cost.md`); past this, rustic streams them.
pub(crate) const PREFETCH_LIMIT: u64 = 256 * 1024 * 1024;

/// How far past its listed size an index file may run before the prefetch lets
/// go of it.
const CAP_SLACK: u64 = 64 * 1024;

/// The longest a waiter goes between two progress ticks it forwards, so a pane
/// hears a few a second rather than one per chunk read.
const TICK_EVERY: Duration = Duration::from_millis(100);

/// How a load ended, once it has.
type Outcome = Option<Result<Arc<IndexedRepo>, ResticError>>;

/// What one load reads, and with what.
pub(crate) struct LoadSpec {
    pub(crate) parent: Arc<dyn Volume>,
    /// The repo's folder on `parent`.
    pub(crate) root: PathBuf,
    pub(crate) password: Zeroizing<String>,
    pub(crate) runtime: Handle,
    /// The most index bytes to read ahead of rustic ([`PREFETCH_LIMIT`]).
    pub(crate) prefetch_limit: u64,
}

/// One index load in flight, shared by every listing waiting on it.
pub(crate) struct Load {
    cancel: CancellationToken,
    /// How many [`Waiter`]s hold it. Joining and leaving both go through this
    /// lock, so a waiter can't join a load the last one just cancelled.
    waiters: Mutex<usize>,
    /// How far it got. Its own channel, ❗ apart from `outcome`: the loaded
    /// repo's adapter keeps the tally (and so this sender) for good, and an
    /// outcome in the same channel would make the repo hold itself.
    progress: watch::Sender<Option<ListingPhase>>,
    outcome: watch::Sender<Outcome>,
    parent: Arc<dyn Volume>,
}

impl Load {
    /// Starts loading on `spec.runtime`. `finished` runs with the outcome before
    /// any waiter hears it, so a waiter that wakes finds the result installed.
    pub(crate) fn start<F>(spec: LoadSpec, finished: F) -> Arc<Self>
    where
        F: FnOnce(&Result<Arc<IndexedRepo>, ResticError>) + Send + 'static,
    {
        let load = Arc::new(Self {
            cancel: CancellationToken::new(),
            waiters: Mutex::new(0),
            progress: watch::channel(None).0,
            outcome: watch::channel(None).0,
            parent: Arc::clone(&spec.parent),
        });
        let task_load = Arc::clone(&load);
        let runtime = spec.runtime.clone();
        // An outer task around the load, so a load that panics still answers its
        // waiters (a typed error) instead of leaving them waiting forever.
        drop(runtime.spawn(async move {
            let cancel = task_load.cancel.clone();
            let inner_runtime = spec.runtime.clone();
            let progress = task_load.progress.clone();
            let outcome = inner_runtime
                .spawn(run(spec, cancel, progress))
                .await
                .map_err(worker_crashed)
                .and_then(|outcome| outcome)
                .map(Arc::new);
            finished(&outcome);
            task_load.outcome.send_replace(Some(outcome));
        }));
        load
    }

    /// Whether this load reads through `parent` (a replaced parent starts anew).
    pub(crate) fn is_over(&self, parent: &Arc<dyn Volume>) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.parent), Arc::as_ptr(parent))
    }

    /// A place among the load's waiters, or `None` once it was cancelled (every
    /// waiter left before it finished), when the caller starts a fresh one.
    pub(crate) fn join(self: &Arc<Self>) -> Option<Waiter> {
        let mut waiters = lock(&self.waiters);
        if self.cancel.is_cancelled() {
            return None;
        }
        *waiters += 1;
        Some(Waiter {
            load: Arc::clone(self),
            last_tick: Mutex::new(None),
        })
    }

    /// A waiter left. The last one to leave a load that hasn't finished cancels
    /// it: nobody wants the index any more, and every read it would still make
    /// is a NAS round trip for nothing.
    fn leave(&self) {
        let mut waiters = lock(&self.waiters);
        *waiters = waiters.saturating_sub(1);
        if *waiters == 0 && self.outcome.borrow().is_none() {
            self.cancel.cancel();
        }
    }

    /// How many waiters hold it.
    #[cfg(test)]
    pub(crate) fn waiters(&self) -> usize {
        *lock(&self.waiters)
    }
}

/// One listing (or stat, or read) waiting on a [`Load`]. Dropping it is
/// leaving: a pane's cancel drops the listing's future, and with it this.
pub(crate) struct Waiter {
    load: Arc<Load>,
    last_tick: Mutex<Option<Instant>>,
}

impl Drop for Waiter {
    fn drop(&mut self) {
        self.load.leave();
    }
}

impl Waiter {
    /// Waits for the load's outcome, handing `on_progress` its phase as the
    /// bytes arrive (a few times a second at most).
    pub(crate) async fn outcome(
        &self,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Arc<IndexedRepo>, ResticError> {
        let mut outcome = self.load.outcome.subscribe();
        let mut progress = self.load.progress.subscribe();
        loop {
            let ended = outcome.borrow_and_update().clone();
            if let Some(ended) = ended {
                return ended;
            }
            let phase = *progress.borrow_and_update();
            if let (Some(phase), Some(report)) = (phase, on_progress)
                && self.tick_due()
            {
                report(ListingProgress::in_phase(phase));
            }
            // Both senders live in the load this waiter holds, so neither
            // channel closes while it waits.
            tokio::select! {
                _ = outcome.changed() => {}
                _ = progress.changed() => {}
            }
        }
    }

    fn tick_due(&self) -> bool {
        let mut last = lock(&self.last_tick);
        let due = last.is_none_or(|at| at.elapsed() >= TICK_EVERY);
        if due {
            *last = Some(Instant::now());
        }
        due
    }
}

/// The load itself: list `index/`, read the files ahead of rustic (when small
/// enough) while the repo opens beside it, then build the index.
async fn run(
    spec: LoadSpec,
    cancel: CancellationToken,
    progress: watch::Sender<Option<ListingPhase>>,
) -> Result<IndexedRepo, ResticError> {
    let index_dir = spec.root.join(FileType::Index.dirname());
    // On its own task, so a cancel DETACHES the listing (dropping the handle),
    // ❌ never drops it mid-transaction: that wedges a phone (MTP).
    let listing = {
        let (parent, dir) = (Arc::clone(&spec.parent), index_dir.clone());
        spec.runtime
            .spawn(async move { index_files(parent.as_ref(), &dir).await })
    };
    let files = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(cancelled(&index_dir)),
        files = listing => files.map_err(worker_crashed)?.map_err(ResticError::Volume)?,
    };
    let total_bytes: u64 = files.iter().map(|(_, size)| size).sum();
    let feed = Arc::new(IndexFeed::new(
        cancel.clone(),
        total_bytes,
        u32::try_from(files.len()).unwrap_or(u32::MAX),
        progress,
    ));
    let backend = Arc::new(
        VolumeBackend::new(Arc::clone(&spec.parent), spec.root.clone(), spec.runtime.clone())
            .with_index_feed(Arc::clone(&feed)),
    );
    // The unlock (scrypt, about half a second) runs beside the prefetch.
    let opening = {
        let backend = Arc::clone(&backend);
        let password = spec.password;
        spec.runtime
            .spawn_blocking(move || open_blocking(&backend, &password, None))
    };
    if total_bytes <= spec.prefetch_limit {
        prefetch(
            &spec.parent,
            &index_dir,
            &files,
            spec.prefetch_limit,
            &feed,
            &spec.runtime,
        )
        .await
        .map_err(ResticError::Volume)?;
    }
    let open = opening.await.map_err(worker_crashed)??;
    if cancel.is_cancelled() {
        return Err(cancelled(&index_dir));
    }
    let indexed = spec
        .runtime
        .spawn_blocking(move || IndexedRepo::index(open, backend))
        .await
        .map_err(worker_crashed)?;
    // Anything rustic didn't ask for (a file gone between the two listings).
    feed.clear();
    indexed
}

/// The index files in `dir`, by id, with their sizes. A missing folder is an
/// empty index.
async fn index_files(parent: &dyn Volume, dir: &Path) -> Result<Vec<(Id, u64)>, VolumeError> {
    let entries = match parent.list_directory(dir, None).await {
        Ok(entries) => entries,
        Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    Ok(entries
        .into_iter()
        .filter(|e| !e.is_directory)
        .filter_map(|e| Some((e.name.parse::<Id>().ok()?, e.size.unwrap_or(0))))
        .collect())
}

/// Reads every index file into `feed`, [`PREFETCH_CONCURRENCY`] at a time.
///
/// A cancel answers at once, and each read in flight stops at its next chunk.
/// ❌ Never aborted mid-read: a read dropped inside a device transaction (a repo
/// on a phone) can wedge the device, so the reads are detached, not aborted.
async fn prefetch(
    parent: &Arc<dyn Volume>,
    dir: &Path,
    files: &[(Id, u64)],
    budget: u64,
    feed: &Arc<IndexFeed>,
    runtime: &Handle,
) -> Result<(), VolumeError> {
    // A file is held up to its listed size plus some slack, or the whole budget
    // when the listing gave no size.
    let cap_of = move |listed: u64| if listed > 0 { listed + CAP_SLACK } else { budget };
    let mut pending = files.iter().map(|(id, listed)| (*id, cap_of(*listed)));
    let mut running = JoinSet::new();
    loop {
        while running.len() < PREFETCH_CONCURRENCY {
            let Some((id, cap)) = pending.next() else { break };
            let read = read_one(Arc::clone(parent), dir.join(&*id.to_hex()), id, cap, Arc::clone(feed));
            running.spawn_on(read, runtime);
        }
        let next = tokio::select! {
            biased;
            () = feed.cancel().cancelled() => {
                running.detach_all();
                return Err(VolumeError::Cancelled(dir.display().to_string().into()));
            }
            next = running.join_next() => next,
        };
        let failed = match next {
            None => return Ok(()),
            Some(Ok(Ok(()))) => continue,
            Some(Ok(Err(err))) => err,
            Some(Err(crashed)) => VolumeError::IoError {
                message: format!("an index read stopped: {crashed}").into(),
                raw_os_error: None,
            },
        };
        running.detach_all();
        return Err(failed);
    }
}

/// Reads one index file whole into `feed`, counting each chunk as it lands.
///
/// Holds at most `cap` bytes: past it, the file is let go of (rustic reads it
/// itself), so a listing that under-reports sizes (or reports none) never makes
/// the prefetch hold more than it budgeted.
async fn read_one(
    parent: Arc<dyn Volume>,
    path: PathBuf,
    id: Id,
    cap: u64,
    feed: Arc<IndexFeed>,
) -> Result<(), VolumeError> {
    let mut stream = parent.open_read_stream(&path).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        if feed.is_cancelled() {
            return Err(VolumeError::Cancelled(path.display().to_string().into()));
        }
        let chunk = chunk?;
        feed.add_bytes(chunk.len());
        bytes.extend_from_slice(&chunk);
        if bytes.len() as u64 > cap {
            return Ok(());
        }
    }
    feed.put(id, Bytes::from(bytes));
    feed.file_done();
    Ok(())
}

fn cancelled(path: &Path) -> ResticError {
    ResticError::Volume(VolumeError::Cancelled(path.display().to_string().into()))
}

/// Locks a std mutex, taking the data over from a thread that panicked holding
/// it: every value here stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
