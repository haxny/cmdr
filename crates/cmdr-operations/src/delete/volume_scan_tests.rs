//! The volume scan's progress throttle.

use super::*;

/// A panic that poisoned the throttle's timestamp leaves a timestamp: the scan
/// keeps reporting its tally instead of going silent for the rest of the walk.
#[test]
fn a_poisoned_throttle_still_lets_progress_through() {
    let tracker = VolumeScanTracker::new(Duration::ZERO);
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = tracker.last_emit.lock();
        panic!("a progress callback panicked, on purpose");
    }));
    assert!(poisoned.is_err() && tracker.last_emit.is_poisoned());

    assert!(tracker.should_emit(), "a due tick still goes out");
}
