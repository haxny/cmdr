//! Before an eject that takes several volumes down: is anyone holding one of them?
//!
//! `diskutil eject` isn't atomic. It unmounts what it can and stops at a held volume, so a
//! refusal alone leaves a half-ejected disk (verified on macOS 27.0.1, a GPT stick with one APFS
//! and one exFAT volume, 2026-10-30). Asking first turns most of those into a refusal with
//! nothing unmounted.
//!
//! It runs AFTER the index stop, so Cmdr's own index can't read as a holder, and it can only
//! reduce the risk: a holder can arrive between the scan and the tool, and the walk can't see a
//! root-owned process (`volume/DETAILS.md` § "A refusal names who held the drive"). What's left
//! stays the honest `UnmountRefused`.

use std::path::PathBuf;
use std::time::Duration;

use super::holders::{self, HolderKind, HolderScan, VolumeHolder};
use super::unmount_tool::REFUSAL_RETRY_BACKOFF;

/// Whether `holder` keeps the disk up, so the eject should refuse before it unmounts anything.
///
/// ❌ Not `System`: LaunchServices and Spotlight let go on their own as the unmount starts, and
/// the tool's own retries wait them out. ❌ Not `Unclassified`: the budget ran out or a signature
/// wouldn't read, so it says nothing, and refusing over a daemon nobody can close strands the
/// drive. ❌ Not this process: the approver lets go of the repos on the drive only when asked.
/// A process Cmdr STARTED has nobody to ask, so it does block.
fn blocks_the_eject(holder: &VolumeHolder, own_pid: u32) -> bool {
    match holder.kind {
        HolderKind::App | HolderKind::Tool | HolderKind::DiskImage => true,
        HolderKind::Cmdr => holder.pid != own_pid,
        HolderKind::System | HolderKind::Unclassified => false,
    }
}

/// Whether anything `scan` named keeps the disk up.
fn is_blocked(scan: &HolderScan, own_pid: u32) -> bool {
    scan.named().iter().any(|holder| blocks_the_eject(holder, own_pid))
}

/// Who's keeping the disk up, or `None` when the eject may go ahead. A holder has to outlast the
/// same pauses a refused unmount gets, since a short hold shouldn't refuse an eject that would
/// have worked. `scan` is a parameter so a test scripts it on a paused clock.
async fn held_through_the_pauses<F, Fut>(own_pid: u32, mut scan: F) -> Option<HolderScan>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = HolderScan>,
{
    let mut latest = scan().await;
    for pause in REFUSAL_RETRY_BACKOFF {
        if !is_blocked(&latest, own_pid) {
            return None;
        }
        tokio::time::sleep(pause).await;
        latest = scan().await;
    }
    is_blocked(&latest, own_pid).then_some(latest)
}

/// Who's keeping the volumes mounted at `paths` up, or `None` when nobody the scan could name is.
///
/// ❗ `None` is "go ahead", ❌ never "nobody is holding it": an incomplete scan that named no
/// blocker also answers it, because refusing over what nobody knows would strand the drive.
pub(super) async fn held_before_eject(paths: &[PathBuf], budget: Duration) -> Option<HolderScan> {
    let listed: Vec<PathBuf> = paths
        .iter()
        .filter(|path| super::is_still_mounted(&path.to_string_lossy()))
        .cloned()
        .collect();
    held_through_the_pauses(std::process::id(), || holders::scan(listed.clone(), budget)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const OWN_PID: u32 = 4242;

    fn holder(kind: HolderKind, pid: u32) -> VolumeHolder {
        VolumeHolder {
            pid,
            name: "someone".to_string(),
            bundle_id: None,
            kind,
        }
    }

    fn complete(named: Vec<VolumeHolder>) -> HolderScan {
        HolderScan::Complete { named }
    }

    #[test]
    fn only_a_holder_someone_can_close_keeps_the_disk_up() {
        for kind in [HolderKind::App, HolderKind::Tool, HolderKind::DiskImage] {
            assert!(blocks_the_eject(&holder(kind, 1), OWN_PID), "{kind:?} blocks");
        }
        for kind in [HolderKind::System, HolderKind::Unclassified] {
            assert!(!blocks_the_eject(&holder(kind, 1), OWN_PID), "{kind:?} doesn't");
        }
    }

    #[test]
    fn this_process_doesnt_block_but_a_process_it_started_does() {
        // The approver lets go of this process's own repos when DiskArbitration asks. Nobody asks
        // a child of ours for anything.
        assert!(!blocks_the_eject(&holder(HolderKind::Cmdr, OWN_PID), OWN_PID));
        assert!(blocks_the_eject(&holder(HolderKind::Cmdr, OWN_PID + 1), OWN_PID));
    }

    #[tokio::test(start_paused = true)]
    async fn a_free_disk_goes_ahead_after_one_scan() {
        let scans = AtomicUsize::new(0);
        let held = held_through_the_pauses(OWN_PID, || {
            scans.fetch_add(1, Ordering::SeqCst);
            std::future::ready(complete(vec![holder(HolderKind::System, 1)]))
        })
        .await;
        assert_eq!(held, None);
        assert_eq!(scans.load(Ordering::SeqCst), 1, "nothing to wait out");
    }

    #[tokio::test(start_paused = true)]
    async fn a_scan_that_couldnt_finish_doesnt_refuse_over_what_it_didnt_see() {
        let held = held_through_the_pauses(OWN_PID, || std::future::ready(HolderScan::not_scanned())).await;
        assert_eq!(
            held, None,
            "an unknown goes ahead, and the tool's own answer stays honest"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_short_hold_doesnt_refuse_an_eject_that_would_have_worked() {
        let scans = AtomicUsize::new(0);
        let held = held_through_the_pauses(OWN_PID, || {
            let scan = scans.fetch_add(1, Ordering::SeqCst);
            std::future::ready(complete(if scan == 0 {
                vec![holder(HolderKind::App, 1)]
            } else {
                Vec::new()
            }))
        })
        .await;
        assert_eq!(held, None);
        assert_eq!(scans.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_holder_that_outlasts_every_pause_refuses_and_says_who() {
        let scans = AtomicUsize::new(0);
        let held = held_through_the_pauses(OWN_PID, || {
            scans.fetch_add(1, Ordering::SeqCst);
            std::future::ready(complete(vec![holder(HolderKind::App, 1)]))
        })
        .await;
        assert_eq!(held, Some(complete(vec![holder(HolderKind::App, 1)])));
        assert_eq!(scans.load(Ordering::SeqCst), REFUSAL_RETRY_BACKOFF.len() + 1);
    }
}
