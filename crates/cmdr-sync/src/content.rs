//! Byte-by-byte compare of pairs whose sizes already match.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cmdr_fs::volume::{BackendKind, Volume, VolumeReadStream};
use futures_util::StreamExt;

use crate::{ContentState, SyncControl};

/// Workers when a network or device sits on either side: round trips overlap.
const REMOTE_WORKERS: usize = 4;

/// One pair to compare.
#[derive(Debug, Clone)]
pub struct ContentJob {
    /// The caller's row id, echoed back with the answer.
    pub item: u32,
    /// The left file, in the left volume's own spelling.
    pub left: PathBuf,
    /// The right file, in the right volume's own spelling.
    pub right: PathBuf,
    /// The size both sides listed.
    pub size: u64,
}

/// Running totals, reported as bytes are read and as pairs finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentProgress {
    /// Pairs decided.
    pub files_done: u64,
    /// Pairs asked for.
    pub files_total: u64,
    /// Bytes accounted for. A pair that differs early counts in full.
    pub bytes_done: u64,
    /// Sizes of all pairs added up.
    pub bytes_total: u64,
}

/// Two local disks seek against each other, so one pair at a time; anything else
/// waits on the wire and gains from overlapping.
pub fn content_workers(left: &dyn Volume, right: &dyn Volume) -> usize {
    if left.backend_kind() == BackendKind::Local && right.backend_kind() == BackendKind::Local {
        1
    } else {
        REMOTE_WORKERS
    }
}

struct Totals {
    files_done: AtomicU64,
    bytes_done: AtomicU64,
    files_total: u64,
    bytes_total: u64,
}

impl Totals {
    fn snapshot(&self) -> ContentProgress {
        ContentProgress {
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            bytes_done: self.bytes_done.load(Ordering::Relaxed).min(self.bytes_total),
            bytes_total: self.bytes_total,
        }
    }
}

/// Compares every job, `workers` at a time. Every job gets an answer: a pair
/// the control stopped before or during its compare answers `Pending`.
pub async fn compare_contents(
    left: &dyn Volume,
    right: &dyn Volume,
    jobs: Vec<ContentJob>,
    workers: usize,
    control: &Arc<SyncControl>,
    on_progress: &(dyn Fn(ContentProgress) + Sync),
) -> Vec<(u32, ContentState)> {
    let totals = Totals {
        files_done: AtomicU64::new(0),
        bytes_done: AtomicU64::new(0),
        files_total: jobs.len() as u64,
        bytes_total: jobs.iter().map(|job| job.size).sum(),
    };
    let totals = &totals;
    futures_util::stream::iter(jobs)
        .map(|job| async move {
            let (state, read) = run_job(left, right, &job, control, totals, on_progress).await;
            if state != ContentState::Pending {
                // A pair decided early still fills its share of the bar.
                totals
                    .bytes_done
                    .fetch_add(job.size.saturating_sub(read), Ordering::Relaxed);
                totals.files_done.fetch_add(1, Ordering::Relaxed);
                on_progress(totals.snapshot());
            }
            (job.item, state)
        })
        .buffer_unordered(workers.max(1))
        .collect()
        .await
}

async fn run_job(
    left: &dyn Volume,
    right: &dyn Volume,
    job: &ContentJob,
    control: &SyncControl,
    totals: &Totals,
    on_progress: &(dyn Fn(ContentProgress) + Sync),
) -> (ContentState, u64) {
    if !may_continue(control).await {
        return (ContentState::Pending, 0);
    }
    let (Some(mut a), Some(mut b)) = (
        open(left, &job.left, job.size).await,
        open(right, &job.right, job.size).await,
    ) else {
        // A failure that follows a cancel is the cancel's, not the file's.
        let state = if control.is_cancelled() {
            ContentState::Pending
        } else {
            ContentState::Unreadable
        };
        return (state, 0);
    };
    let mut read = 0u64;
    let mut sides = (Cursor::default(), Cursor::default());
    loop {
        if !may_continue(control).await {
            return (ContentState::Pending, read);
        }
        for (stream, cursor) in [(&mut a, &mut sides.0), (&mut b, &mut sides.1)] {
            if let Err(state) = cursor.fill(stream.as_mut(), control).await {
                return (state, read);
            }
        }
        let (n, m) = (sides.0.available(), sides.1.available());
        if n == 0 || m == 0 {
            let state = if n == 0 && m == 0 {
                ContentState::Same
            } else {
                ContentState::Differs
            };
            return (state, read);
        }
        let take = n.min(m);
        if sides.0.peek(take) != sides.1.peek(take) {
            return (ContentState::Differs, read);
        }
        sides.0.consume(take);
        sides.1.consume(take);
        read += take as u64;
        totals.bytes_done.fetch_add(take as u64, Ordering::Relaxed);
        on_progress(totals.snapshot());
    }
}

/// `false` once cancelled; waits out a pause first.
async fn may_continue(control: &SyncControl) -> bool {
    control.wait_while_paused().await;
    !control.is_cancelled()
}

async fn open(volume: &dyn Volume, path: &Path, size: u64) -> Option<Box<dyn VolumeReadStream>> {
    volume.open_read_stream_with_hint(path, Some(size)).await.ok()
}

/// The unconsumed part of the chunk a stream last handed over. The two streams'
/// chunk sizes differ, so each side keeps its own window.
#[derive(Default)]
struct Cursor {
    chunk: Vec<u8>,
    at: usize,
    finished: bool,
}

impl Cursor {
    fn available(&self) -> usize {
        self.chunk.len() - self.at
    }

    fn peek(&self, n: usize) -> &[u8] {
        &self.chunk[self.at..self.at + n]
    }

    fn consume(&mut self, n: usize) {
        self.at += n;
    }

    /// Reads until bytes are waiting or the stream ends. `Err(Unreadable)` when
    /// the stream breaks.
    async fn fill(&mut self, stream: &mut dyn VolumeReadStream, control: &SyncControl) -> Result<(), ContentState> {
        while self.available() == 0 && !self.finished {
            // Inside the loop, so a stream of empty chunks can't outlast a cancel.
            if !may_continue(control).await {
                return Err(ContentState::Pending);
            }
            match stream.next_chunk().await {
                Some(Ok(chunk)) => {
                    self.chunk = chunk;
                    self.at = 0;
                }
                Some(Err(_)) if control.is_cancelled() => return Err(ContentState::Pending),
                Some(Err(_)) => return Err(ContentState::Unreadable),
                None => self.finished = true,
            }
        }
        Ok(())
    }
}
