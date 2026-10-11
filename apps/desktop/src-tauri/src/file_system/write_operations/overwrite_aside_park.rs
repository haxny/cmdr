//! Test seam: holds a safe-overwrite open between the rename that sets the
//! user's original ASIDE and the rename that replaces it.
//!
//! ❗ That window is the whole reason the ledger records asides: a crash or a
//! pulled drive inside it leaves the only copy of the file wearing a scratch
//! name. Nothing else can stop an overwrite there — the two renames are
//! back-to-back syscalls.
//!
//! Process-global, not thread-local like the mount-table hook: the engine runs
//! inside `spawn_blocking`, so the overwrite is on a different thread from the
//! test that armed the park. One park at a time; the guard disarms on drop, and
//! a parked overwrite gives up on its own after [`PARK_CAP`] so a
//! test that dies can't wedge the suite.

// Only the macOS real-detach lane arms the park; elsewhere the engine's own hook is
// all that's read, and `-D unused` is fatal on the Linux build.
#![cfg_attr(
    not(target_os = "macos"),
    expect(dead_code, reason = "only the macOS real-detach lane arms the park")
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long a parked overwrite waits for its release before carrying on.
pub(crate) const PARK_CAP: Duration = Duration::from_secs(30);

static ARMED: AtomicBool = AtomicBool::new(false);
static IS_PARKED: AtomicBool = AtomicBool::new(false);
static RELEASED: AtomicBool = AtomicBool::new(false);

/// Arms the park. Disarms on drop, released or not.
pub(crate) struct AsidePark;

/// Parks the next safe-overwrite that has set an original aside.
pub(crate) fn park_next_overwrite() -> AsidePark {
    IS_PARKED.store(false, Ordering::SeqCst);
    RELEASED.store(false, Ordering::SeqCst);
    ARMED.store(true, Ordering::SeqCst);
    AsidePark
}

impl AsidePark {
    /// Blocks until an overwrite is parked with its aside on disk.
    /// `false` if nothing parked within `timeout`.
    pub(crate) fn wait_until_parked(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if IS_PARKED.load(Ordering::SeqCst) {
                return true;
            }
            // allowed-test-sleep: polling a flag another THREAD sets is the
            // subject. The park is what this caller is waiting to observe, so
            // there's no condition `wait_until` could ask about instead.
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Lets the parked overwrite carry on.
    pub(crate) fn release(&self) {
        RELEASED.store(true, Ordering::SeqCst);
    }
}

impl Drop for AsidePark {
    fn drop(&mut self) {
        ARMED.store(false, Ordering::SeqCst);
        RELEASED.store(true, Ordering::SeqCst);
        IS_PARKED.store(false, Ordering::SeqCst);
    }
}

pub(super) fn park_if_armed() {
    // One park per arming: the overwrite that got here owns it.
    if !ARMED.swap(false, Ordering::SeqCst) {
        return;
    }
    IS_PARKED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + PARK_CAP;
    while !RELEASED.load(Ordering::SeqCst) && Instant::now() < deadline {
        // allowed-test-sleep: this IS the park. Holding the overwrite still
        // between the two renames is the whole point, and `PARK_CAP` above
        // keeps a dead test from wedging the suite.
        std::thread::sleep(Duration::from_millis(10));
    }
    IS_PARKED.store(false, Ordering::SeqCst);
}
