//! The copy-scan boundary's two contracts: what it counts, and what it stops.
//!
//! It lives here rather than beside a backend because every remote backend
//! shares the one boundary: a per-backend copy would let the cumulative-counts
//! promise drift between them, and a per-backend stop would let one of them
//! quietly not have one.

use super::*;
use crate::volume::ScanStopSignal;
use crate::volume::scan_stop::TestScanStop;
use std::path::Path;
use std::sync::Arc;

use crate::testing::scan_recording::{Recorded, ScanRecording};

/// The counts a scan reports must be CUMULATIVE for the call: the caller shifts
/// them by its own baseline across several calls, so a per-entry (or per-path)
/// reset would make the dialog's counters jump backwards.
#[tokio::test]
async fn the_boundary_reports_running_totals() {
    use crate::ignore_poison::IgnorePoison;
    use std::sync::Mutex;

    let seen: Mutex<Vec<ListingProgress>> = Mutex::new(Vec::new());
    let record = |p: ListingProgress| seen.lock_ignore_poison().push(p);
    let boundary = ScanBoundary::new(Some(&record));

    boundary
        .dir(Path::new("/top"))
        .await
        .expect("nothing is stopping this walk");
    boundary
        .file(Path::new("/top/a"), 1_000)
        .await
        .expect("nothing is stopping this walk");
    boundary
        .file(Path::new("/top/b"), 24)
        .await
        .expect("nothing is stopping this walk");

    let seen = seen.lock_ignore_poison();
    assert_eq!(seen.len(), 3, "every entry reports, so a slow walk still looks alive");
    assert_eq!((seen[0].files, seen[0].dirs, seen[0].bytes), (0, 1, 0));
    assert_eq!((seen[1].files, seen[1].dirs, seen[1].bytes), (1, 1, 1_000));
    assert_eq!((seen[2].files, seen[2].dirs, seen[2].bytes), (2, 1, 1_024));
}

/// A scan with nobody listening still counts, so `scan_for_copy` (which the
/// trait gives no callback) can share one implementation with the batch path.
#[tokio::test]
async fn a_silent_boundary_still_counts() {
    let boundary = ScanBoundary::silent();
    boundary
        .dir(Path::new("/top"))
        .await
        .expect("a silent boundary stops nothing");
    boundary
        .file(Path::new("/top/a"), 512)
        .await
        .expect("a silent boundary stops nothing");

    let counts = boundary.counts();
    assert_eq!((counts.files, counts.dirs, counts.bytes), (1, 1, 512));
}

/// Both entry calls carry the stop, so a walk can't reach one shape of entry
/// without passing the boundary.
#[tokio::test]
async fn a_stopping_boundary_refuses_both_kinds_of_entry() {
    let signal = TestScanStop::already_stopping();
    let stop = ScanStop::new(Arc::clone(&signal) as Arc<dyn ScanStopSignal>);

    let boundary = ScanBoundary::silent().stopping_at(stop.clone());
    assert!(
        matches!(boundary.dir(Path::new("/top")).await, Err(VolumeError::Cancelled(_))),
        "a directory boundary must refuse"
    );
    assert!(
        matches!(
            boundary.file(Path::new("/top/a"), 1).await,
            Err(VolumeError::Cancelled(_))
        ),
        "a file boundary must refuse"
    );
    assert!(
        matches!(
            ScanBoundary::silent().stopping_at(stop).check().await,
            Err(VolumeError::Cancelled(_))
        ),
        "and so must the bare check a backend uses between groups"
    );
}

/// The counts still climb across a stopped boundary: a caller that reports what
/// it got so far (the watchdog, the dialog) reads the same field either way.
#[tokio::test]
async fn a_stopped_boundary_still_counted_the_entry_it_refused() {
    let signal = TestScanStop::already_stopping();
    let boundary = ScanBoundary::silent().stopping_at(ScanStop::new(signal as Arc<dyn ScanStopSignal>));

    let _ = boundary.file(Path::new("/top/a"), 64).await;
    assert_eq!(boundary.counts().files, 1);
    assert_eq!(boundary.counts().bytes, 64);
}

/// `stop()` hands the signal out for a walk that can't `.await` — the local
/// backend's blocking `WalkDir` loop.
#[tokio::test]
async fn the_boundary_hands_its_stop_to_a_blocking_walk() {
    let signal = TestScanStop::already_stopping();
    let boundary = ScanBoundary::silent().stopping_at(ScanStop::new(signal as Arc<dyn ScanStopSignal>));
    assert!(boundary.stop().should_stop_blocking());
    assert!(!ScanBoundary::silent().stop().should_stop_blocking());
}

/// A recorder on the boundary sees every entry the walk counts, in the path
/// form the walk passed, so a transfer can draw the selection as a tree.
#[tokio::test]
async fn a_recording_boundary_hands_every_entry_to_its_recorder() {
    let recording = Arc::new(ScanRecording::default());
    let boundary = ScanBoundary::silent().recording_to(Arc::clone(&recording) as Arc<dyn ScanRecorder>);

    boundary
        .dir(Path::new("/top"))
        .await
        .expect("nothing is stopping this walk");
    boundary
        .file(Path::new("/top/a"), 7)
        .await
        .expect("nothing is stopping this walk");
    let opaque = CopyScanResult {
        file_count: 3,
        dir_count: 1,
        total_bytes: 30,
        dedup_bytes: 30,
        top_level_is_directory: true,
        top_level_modified_at: None,
        top_level_created_at: None,
    };
    boundary
        .subtree(Path::new("/other"), &opaque)
        .await
        .expect("nothing is stopping this walk");

    assert_eq!(
        recording.sorted(),
        vec![
            Recorded::Dir("/top".into()),
            Recorded::File("/top/a".into(), 7),
            Recorded::Subtree("/other".into(), 3, 30, true),
        ]
    );
    let counts = boundary.counts();
    assert_eq!((counts.files, counts.dirs, counts.bytes), (4, 2, 37));
}

/// A walk that recorded its entries itself (off the runtime, where it can't
/// await the boundary) folds its totals in without recording the subtree a
/// second time as an opaque one.
#[tokio::test]
async fn counted_totals_add_up_without_reaching_the_recorder() {
    let recording = Arc::new(ScanRecording::default());
    let boundary = ScanBoundary::silent().recording_to(Arc::clone(&recording) as Arc<dyn ScanRecorder>);
    let walked = CopyScanResult {
        file_count: 2,
        dir_count: 1,
        total_bytes: 9,
        dedup_bytes: 9,
        top_level_is_directory: true,
        top_level_modified_at: None,
        top_level_created_at: None,
    };

    boundary.counted(&walked).await.expect("nothing is stopping this walk");

    assert!(recording.sorted().is_empty());
    assert!(boundary.recorder().is_some(), "the walk reaches the recorder itself");
    let counts = boundary.counts();
    assert_eq!((counts.files, counts.dirs, counts.bytes), (2, 1, 9));
}
