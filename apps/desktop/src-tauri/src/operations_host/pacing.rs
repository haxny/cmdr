//! The write engine's E2E pacing, answered by `test_mode`: the IPC overrides a
//! spec sets, then the `CMDR_E2E_*` variables, then nothing. A shipped build
//! sets neither, so every answer is `None`.

use std::path::Path;
use std::time::Duration;

use crate::file_system::write_operations::host::Pacing;
use crate::test_mode;

/// `test_mode`'s throttles, read per call: a spec changes them mid-run.
pub(super) struct AppPacing;

impl Pacing for AppPacing {
    fn copy_throttle(&self) -> Option<Duration> {
        pause(test_mode::effective_copy_throttle_ms())
    }

    fn copy_throttle_for(&self, source: &Path) -> Option<Duration> {
        pause(test_mode::effective_copy_throttle_ms_for(source))
    }

    fn scan_preview_delay(&self) -> Option<Duration> {
        pause(test_mode::e2e_scan_preview_delay_ms())
    }

    fn rollback_pace(&self) -> Option<Duration> {
        pause(test_mode::effective_rollback_throttle_ms())
    }
}

/// A throttle in milliseconds as the seam speaks it: zero is no pause.
fn pause(ms: Option<u64>) -> Option<Duration> {
    ms.filter(|ms| *ms > 0).map(Duration::from_millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spec's rollback pace reaches the engine while it's set, and is gone the
    /// moment it's cleared. Both reads happen under the pacing lock, so a
    /// sibling's pace can't land in between.
    #[tokio::test]
    async fn the_rollback_pace_follows_test_mode() {
        let _pacing = test_mode::pace_rollback_for_test(25).await;
        assert_eq!(AppPacing.rollback_pace(), Some(Duration::from_millis(25)));
        test_mode::set_rollback_throttle_override(Some(0));
        assert_eq!(AppPacing.rollback_pace(), None);
    }

    #[test]
    fn zero_is_no_pause() {
        assert_eq!(pause(Some(0)), None);
        assert_eq!(pause(None), None);
        assert_eq!(pause(Some(40)), Some(Duration::from_millis(40)));
    }
}
