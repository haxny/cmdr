//! [`IndexFeed`]: the bounded window an index load streams the repo's index
//! files through, from the reads on the async runtime to rustic's decoders.
//!
//! A leaf on purpose. The adapter (`backend.rs`) takes from it and the load
//! (`index_load.rs`) fills it, so neither has to name the other's module.
//!
//! rustic asks for the files in its own order: each decoder walks a contiguous
//! run of the listing, and the runs start wherever rayon splits it. So the feed
//! reads ahead (the file after each one asked for, then the listing in order)
//! only while what it holds fits the window, and a file a decoder waits on is
//! read at once, window or not. Each file's bytes go the moment rustic takes
//! them. So it holds at most the window plus one file per decoder, whatever
//! the index's size.

use bytes::Bytes;
use cmdr_fs::volume::{ListingPhase, VolumeError};
use rustic_core::Id;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

/// How many files are read ahead of rustic at once. rustic's own load kept one
/// per core in flight (16 on an M3 Max); four keep a NAS busy without crowding
/// out the browsing the load is for. A file a decoder waits on doesn't count.
const READS_AHEAD: usize = 4;

/// How far past its listed size a file read ahead may run before the feed lets
/// go of it (it's read again when rustic asks).
const CAP_SLACK: u64 = 64 * 1024;

/// One index file to read: where it sits in the listing, and its id.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Start {
    pub(crate) index: usize,
    pub(crate) id: Id,
    /// What to reserve for its bytes before any arrive: its listed size, but
    /// never more than the window, so a hostile or broken listing (a size
    /// field near `u64::MAX`) can't make one read ask for an allocation that
    /// aborts the app. The buffer grows past it as bytes really arrive.
    pub(crate) reserve: u64,
}

/// What a read does after the chunk it just got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    Going,
    /// Stop: the load ended, or a read ahead ran past what the window gave it
    /// (the feed let go of it; rustic's ask reads it again).
    Stop,
}

/// What [`IndexFeed::take`] answers.
pub(crate) enum Taken {
    Bytes(Bytes),
    /// Not a file the feed holds for rustic: read it directly.
    NotFed,
}

/// The index files of one load, between the parent and rustic.
pub(crate) struct IndexFeed {
    files: Vec<(Id, u64)>,
    by_id: HashMap<Id, usize>,
    window: u64,
    state: Mutex<State>,
    /// Wakes rustic's decoders waiting on a file.
    arrived: Condvar,
    /// Wakes the load's reader when something may start.
    startable: Notify,
    /// The load built its index: from here the adapter reads index files
    /// directly, as any adapter does.
    stepped_aside: AtomicBool,
    cancel: CancellationToken,
    tally: Tally,
}

struct State {
    slots: Vec<Slot>,
    /// Files a decoder waits on that nobody reads yet, first come first.
    asked: VecDeque<usize>,
    /// The file after each one asked for: what its decoder wants next.
    hints: VecDeque<usize>,
    /// Every file before this one was started at least once.
    next_in_order: usize,
    reading_ahead: usize,
    /// What the window counts: files ready, plus each read in flight at the
    /// larger of its listed size and what it got.
    committed: u64,
    /// What the feed actually holds: files ready, plus what reads in flight
    /// got so far.
    held: u64,
    peak_held: u64,
    /// The reads are over (failed, or the load ended): a decoder waiting on a
    /// file that won't come hears this.
    ended: Option<VolumeError>,
    served: u32,
    #[cfg(test)]
    read_directly: u32,
    #[cfg(test)]
    serving_threads: Vec<String>,
}

#[derive(Debug)]
enum Slot {
    Unread,
    /// A read ahead that ran past its listed size: read only when asked.
    LetGo,
    Reading {
        /// What the window counts for it so far.
        counted: u64,
        got: u64,
        /// A decoder waits on it, so it's never let go of.
        asked: bool,
    },
    Ready(Bytes),
    Taken,
}

impl IndexFeed {
    /// A feed for `files` (the load's listing of `index/`, ids and sizes) that
    /// reads at most `window` bytes ahead of rustic, and publishes each move of
    /// its tally to `progress` (and the starting zero right away).
    pub(crate) fn new(
        files: Vec<(Id, u64)>,
        window: u64,
        cancel: CancellationToken,
        progress: watch::Sender<Option<ListingPhase>>,
    ) -> Self {
        let tally = Tally {
            done_bytes: AtomicU64::new(0),
            done_files: AtomicU32::new(0),
            total_bytes: files.iter().map(|(_, size)| size).sum(),
            total_files: u32::try_from(files.len()).unwrap_or(u32::MAX),
            state: progress,
        };
        tally.publish();
        let state = State {
            slots: files.iter().map(|_| Slot::Unread).collect(),
            asked: VecDeque::new(),
            hints: VecDeque::new(),
            next_in_order: 0,
            reading_ahead: 0,
            committed: 0,
            held: 0,
            peak_held: 0,
            ended: None,
            served: 0,
            #[cfg(test)]
            read_directly: 0,
            #[cfg(test)]
            serving_threads: Vec::new(),
        };
        Self {
            by_id: files.iter().enumerate().map(|(index, (id, _))| (*id, index)).collect(),
            files,
            window,
            state: Mutex::new(state),
            arrived: Condvar::new(),
            startable: Notify::new(),
            stepped_aside: AtomicBool::new(false),
            cancel,
            tally,
        }
    }

    /// The load's listing, as rustic's `list_with_size` answers it, so rustic
    /// asks for exactly the files the feed reads (and `index/` is listed once).
    pub(crate) fn listing(&self) -> Vec<(Id, u32)> {
        self.files
            .iter()
            .map(|(id, size)| (*id, u32::try_from(*size).unwrap_or(u32::MAX)))
            .collect()
    }

    /// Index file `id`'s bytes, for rustic: blocks its decoder until they're
    /// in. Runs on rustic's threads, ❌ never an async worker. Answers the
    /// reads' failure, or `Cancelled` once every waiter left.
    pub(crate) fn take(&self, id: &Id) -> Result<Taken, VolumeError> {
        if self.cancel.is_cancelled() {
            return Err(VolumeError::Cancelled(format!("index file {id}").into()));
        }
        if self.has_stepped_aside() {
            return Ok(Taken::NotFed);
        }
        let Some(&index) = self.by_id.get(id) else {
            return Ok(Taken::NotFed);
        };
        let mut state = lock(&self.state);
        if let Some(err) = &state.ended {
            return Err(err.clone());
        }
        match state.slots[index] {
            Slot::Unread | Slot::LetGo => state.asked.push_back(index),
            Slot::Reading { asked: false, .. } => {
                if let Slot::Reading { asked, .. } = &mut state.slots[index] {
                    *asked = true;
                }
                state.reading_ahead -= 1;
            }
            // rustic asks for each file once; a second ask reads it directly.
            Slot::Taken => return Ok(Taken::NotFed),
            Slot::Reading { asked: true, .. } | Slot::Ready(_) => {}
        }
        if index + 1 < self.files.len() {
            state.hints.push_back(index + 1);
        }
        self.startable.notify_one();
        loop {
            if self.cancel.is_cancelled() {
                return Err(VolumeError::Cancelled(format!("index file {id}").into()));
            }
            if matches!(state.slots[index], Slot::Ready(_)) {
                let Slot::Ready(bytes) = std::mem::replace(&mut state.slots[index], Slot::Taken) else {
                    unreachable!("matched as ready just above");
                };
                let len = bytes.len() as u64;
                state.held -= len;
                state.committed -= len;
                state.served += 1;
                #[cfg(test)]
                state
                    .serving_threads
                    .push(std::thread::current().name().unwrap_or_default().to_string());
                drop(state);
                // Room in the window: the reader may start the next one.
                self.startable.notify_one();
                return Ok(Taken::Bytes(bytes));
            }
            if let Some(err) = &state.ended {
                return Err(err.clone());
            }
            state = self.arrived.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The next file to read, if one may start now: one a decoder waits on
    /// always may; one read ahead only while fewer than [`READS_AHEAD`] are,
    /// and the window has room for it (or holds nothing). `None` when nothing
    /// may start yet.
    pub(crate) fn next_read(&self) -> Option<Start> {
        let mut state = lock(&self.state);
        if state.ended.is_some() {
            return None;
        }
        while let Some(index) = state.asked.pop_front() {
            if matches!(state.slots[index], Slot::Unread | Slot::LetGo) {
                return Some(self.start(&mut state, index, true));
            }
        }
        if state.reading_ahead >= READS_AHEAD {
            return None;
        }
        let index = loop {
            if let Some(hint) = state.hints.pop_front() {
                if matches!(state.slots[hint], Slot::Unread) {
                    break hint;
                }
                continue;
            }
            let mut next = state.next_in_order;
            while next < self.files.len() && !matches!(state.slots[next], Slot::Unread) {
                next += 1;
            }
            state.next_in_order = next;
            if next == self.files.len() {
                return None;
            }
            break next;
        };
        if state.committed > 0 && state.committed + self.counted_at_start(index) > self.window {
            // No room yet; it's first in line once rustic takes something.
            state.hints.push_front(index);
            return None;
        }
        state.reading_ahead += 1;
        Some(self.start(&mut state, index, false))
    }

    /// What the window counts for file `index` before any of it arrived: its
    /// listed size, or the whole window when the listing gave none.
    fn counted_at_start(&self, index: usize) -> u64 {
        match self.files[index].1 {
            0 => self.window,
            listed => listed,
        }
    }

    fn start(&self, state: &mut State, index: usize, asked: bool) -> Start {
        let counted = self.counted_at_start(index);
        state.committed += counted;
        state.slots[index] = Slot::Reading { counted, got: 0, asked };
        let (id, listed) = self.files[index];
        Start {
            index,
            id,
            reserve: listed.min(self.window),
        }
    }

    /// `bytes` more of file `index` arrived.
    pub(crate) fn add_chunk(&self, index: usize, bytes: usize) -> Keep {
        if self.cancel.is_cancelled() {
            return Keep::Stop;
        }
        let bytes = bytes as u64;
        let mut state = lock(&self.state);
        let Slot::Reading { counted, got, asked } = state.slots[index] else {
            return Keep::Stop;
        };
        let got = got + bytes;
        if !asked && got > self.counted_at_start(index) + CAP_SLACK {
            // Read ahead past its listed size: let go of it, so the window
            // holds what it budgeted. rustic's ask reads it whole.
            let already = got - bytes;
            state.slots[index] = Slot::LetGo;
            state.reading_ahead -= 1;
            state.committed -= counted;
            state.held -= already;
            drop(state);
            self.tally.retract_bytes(already);
            self.startable.notify_one();
            return Keep::Stop;
        }
        let counted_now = counted.max(got);
        state.committed += counted_now - counted;
        state.slots[index] = Slot::Reading {
            counted: counted_now,
            got,
            asked,
        };
        state.held += bytes;
        state.peak_held = state.peak_held.max(state.held);
        drop(state);
        self.tally.add_bytes(bytes);
        Keep::Going
    }

    /// File `index` arrived whole.
    pub(crate) fn deliver(&self, index: usize, bytes: Bytes) {
        let mut state = lock(&self.state);
        let Slot::Reading { counted, asked, .. } = state.slots[index] else {
            return;
        };
        // From here the window counts what arrived, not what was listed.
        state.committed = state.committed - counted + bytes.len() as u64;
        if !asked {
            state.reading_ahead -= 1;
        }
        state.slots[index] = Slot::Ready(bytes);
        drop(state);
        self.tally.file_done();
        self.arrived.notify_all();
        self.startable.notify_one();
    }

    /// The reads ended with `err`: every decoder waiting, or asking later,
    /// hears it.
    pub(crate) fn fail(&self, err: VolumeError) {
        lock(&self.state).ended.get_or_insert(err);
        self.arrived.notify_all();
    }

    /// The load is over (built, failed, or cancelled): nothing more starts, a
    /// decoder still waiting hears `Cancelled`, and whatever rustic never took
    /// is let go of.
    pub(crate) fn close(&self) {
        let mut state = lock(&self.state);
        state
            .ended
            .get_or_insert_with(|| VolumeError::Cancelled("the index load ended".into()));
        for slot in &mut state.slots {
            if !matches!(slot, Slot::Taken) {
                *slot = Slot::Taken;
            }
        }
        state.held = 0;
        state.committed = 0;
        drop(state);
        self.arrived.notify_all();
        self.startable.notify_one();
    }

    /// The load built its index: closes the feed, and the adapter reads any
    /// later index file directly (none, as rustic builds the index once).
    pub(crate) fn step_aside(&self) {
        self.stepped_aside.store(true, Ordering::Release);
        self.close();
    }

    /// Whether the load built its index ([`Self::step_aside`]).
    pub(crate) fn has_stepped_aside(&self) -> bool {
        self.stepped_aside.load(Ordering::Acquire)
    }

    /// Whether the reader has nothing left to do: every file reached rustic's
    /// side (ready or taken), or the load ended.
    pub(crate) fn is_done(&self) -> bool {
        let state = lock(&self.state);
        state.ended.is_some()
            || state
                .slots
                .iter()
                .all(|slot| matches!(slot, Slot::Ready(_) | Slot::Taken))
    }

    /// Resolves once something may start.
    pub(crate) async fn startable(&self) {
        self.startable.notified().await;
    }

    /// The load's cancel: fired when every waiter left.
    pub(crate) fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// rustic read an index file the feed didn't hold, `bytes` long.
    pub(crate) fn note_read(&self, bytes: usize) {
        #[cfg(test)]
        {
            lock(&self.state).read_directly += 1;
        }
        self.tally.add_bytes(bytes as u64);
    }

    /// The most bytes the feed held at once.
    #[cfg(test)]
    pub(crate) fn peak_held_bytes(&self) -> u64 {
        lock(&self.state).peak_held
    }

    /// How many files rustic took from the feed.
    #[cfg(test)]
    pub(crate) fn files_served(&self) -> u32 {
        lock(&self.state).served
    }

    /// How many index files rustic read itself, past the feed.
    #[cfg(test)]
    pub(crate) fn files_read_directly(&self) -> u32 {
        lock(&self.state).read_directly
    }

    /// The thread rustic took each file on.
    #[cfg(test)]
    pub(crate) fn serving_threads(&self) -> Vec<String> {
        lock(&self.state).serving_threads.clone()
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
    fn add_bytes(&self, bytes: u64) {
        self.done_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.publish();
    }

    /// A read ahead the feed let go of: its bytes are read, and counted, again.
    fn retract_bytes(&self, bytes: u64) {
        self.done_bytes.fetch_sub(bytes, Ordering::Relaxed);
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
/// it: the state stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "index_feed_tests.rs"]
mod tests;
