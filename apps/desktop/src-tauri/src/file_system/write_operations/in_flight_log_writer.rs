//! The persisted ledger's writer: one thread per open log, and the only code
//! that writes to it.
//!
//! **Why a thread.** Records arrive from the per-file hot path of every copy,
//! which runs on an async runtime worker. A `write(2)` there, under the
//! ledger's `std` mutex, is file I/O on a thread the whole app shares, and every
//! other copy task queues on the mutex behind it. It's microseconds on a healthy
//! disk, but nothing bounds it (ERR-PTRGY put a 100 s stall inside this
//! neighborhood; the ledger was a suspect and never confirmed). So the store
//! only queues the bytes and this thread writes them, in the order they were
//! queued, which keeps compaction's rewrite consistent with the appends around
//! it.
//!
//! **What it costs.** A record reaches the kernel a moment after `register`
//! returns, not inside it. A crash in that window loses the record, and the
//! hour-gated directory scan (`reap_stale_transfer_temps`) is its backstop: the
//! same one a power loss already relies on (`in_flight_temps.rs` module docs).
//! The quit teardown waits for the queue to drain ([`LogWriter::barrier`]), so
//! an ordinary quit loses nothing.

use std::fs::File;
use std::io::Write;
use std::sync::mpsc;
use std::thread::JoinHandle;

/// What the writer thread is asked to do, in order.
enum Op {
    /// Append one line.
    Append(Vec<u8>),
    /// Truncate the log and write these lines instead: compaction.
    Rewrite(Vec<u8>),
    /// Answer once everything queued before it has reached the kernel.
    Barrier(mpsc::SyncSender<()>),
}

/// The open log, owned by its writer thread.
///
/// Dropping it closes the queue and joins the thread, so every line queued
/// before the drop is written by the time the drop returns. Production never
/// drops one (the process ledger lives for the process); a test's ledger does,
/// which is what makes its log readable afterwards.
pub(super) struct LogWriter {
    /// `Some` until `Drop`, which takes it to close the queue before joining.
    queue: Option<mpsc::Sender<Op>>,
    thread: Option<JoinHandle<()>>,
}

impl LogWriter {
    /// Starts the writer thread for `log`, which must be open in append mode.
    pub(super) fn spawn(log: File) -> std::io::Result<Self> {
        let (queue, ops) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("cmdr-temp-ledger".to_string())
            .spawn(move || write_all_queued(log, &ops))?;
        Ok(Self {
            queue: Some(queue),
            thread: Some(thread),
        })
    }

    /// Queues one line. Best-effort, like every write to this log.
    pub(super) fn append(&self, line: Vec<u8>) {
        self.send(Op::Append(line));
    }

    /// Queues a rewrite of the whole log to `contents`.
    pub(super) fn rewrite(&self, contents: Vec<u8>) {
        self.send(Op::Rewrite(contents));
    }

    /// Queues a marker and hands back what answers once the writer reaches it.
    /// `None` when the writer is gone.
    ///
    /// Returned rather than awaited here so the caller can wait with the
    /// ledger's mutex released.
    pub(super) fn barrier(&self) -> Option<mpsc::Receiver<()>> {
        let (done, reached) = mpsc::sync_channel(1);
        self.queue.as_ref()?.send(Op::Barrier(done)).ok()?;
        Some(reached)
    }

    fn send(&self, op: Op) {
        let sent = self.queue.as_ref().is_some_and(|queue| queue.send(op).is_ok());
        if !sent {
            log::debug!(target: "copy", "couldn't record an in-flight leftover: the ledger's writer is gone");
        }
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        drop(self.queue.take());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::warn!(target: "copy", "the in-flight temp ledger's writer panicked; its last records may be missing");
        }
    }
}

/// The writer thread's body: every op, in order, until the queue closes.
fn write_all_queued(mut log: File, ops: &mpsc::Receiver<Op>) {
    for op in ops {
        match op {
            Op::Append(line) => {
                if let Err(e) = log.write_all(&line) {
                    log::debug!(target: "copy", "couldn't record an in-flight leftover: {e}");
                }
            }
            // The handle is in append mode, so after the truncate the writes go
            // to the new end without a seek.
            Op::Rewrite(contents) => {
                if let Err(e) = log.set_len(0).and_then(|()| log.write_all(&contents)) {
                    log::debug!(target: "copy", "couldn't compact the in-flight temp ledger: {e}");
                }
            }
            Op::Barrier(done) => {
                // A waiter that gave up has nothing left to tell.
                let _ = done.send(());
            }
        }
    }
}
