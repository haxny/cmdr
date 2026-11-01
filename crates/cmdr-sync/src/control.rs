//! Cancel and pause for a running scan or compare.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cmdr_fs::volume::ScanStopSignal;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// How often a blocked thread re-reads the flags (only a walk inside
/// `spawn_blocking` parks a thread; ours never does, but the trait asks).
const BLOCKING_POLL: Duration = Duration::from_millis(5);

/// One cancel switch and one pause switch, shared by everything a comparison runs.
#[derive(Debug)]
pub struct SyncControl {
    cancelled: AtomicBool,
    paused: AtomicBool,
    wake: Notify,
    token: CancellationToken,
}

impl SyncControl {
    /// A control that is neither cancelled nor paused.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            wake: Notify::new(),
            token: CancellationToken::new(),
        })
    }

    /// Stops everything that answers to this control, and releases a pause.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.token.cancel();
        self.wake.notify_waiters();
    }

    /// Holds (`true`) or releases (`false`) everything that waits between steps.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        self.wake.notify_waiters();
    }

    /// Whether `cancel` was called.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// The same cancel as a token, for volume calls that take one.
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Returns at once when not paused; otherwise when the pause lifts or the
    /// control is cancelled.
    pub async fn wait_while_paused(&self) {
        loop {
            // Created before the flags are read, so a wake between the read and
            // the await is not lost.
            let woken = self.wake.notified();
            if !self.paused.load(Ordering::Acquire) || self.is_cancelled() {
                return;
            }
            woken.await;
        }
    }
}

impl ScanStopSignal for SyncControl {
    fn is_stopping_or_paused(&self) -> bool {
        self.is_cancelled() || self.paused.load(Ordering::Acquire)
    }

    fn stop_or_park<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            // Stop outranks pause, and the stop is re-read after waking.
            if self.is_cancelled() {
                return true;
            }
            self.wait_while_paused().await;
            self.is_cancelled()
        })
    }

    fn stop_or_park_blocking(&self) -> bool {
        while self.paused.load(Ordering::Acquire) && !self.is_cancelled() {
            std::thread::sleep(BLOCKING_POLL);
        }
        self.is_cancelled()
    }
}
