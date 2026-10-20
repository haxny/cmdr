//! `report_while`: a slow listing's running count reaches the pane on a timer,
//! a zero count never does, and the listing's own answer comes back untouched.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::ListingProgress;
use super::listing_progress::report_while;

#[tokio::test(start_paused = true)]
async fn a_slow_listing_reports_its_running_count_and_then_its_answer() {
    let listed = AtomicUsize::new(0);
    let seen = Mutex::new(Vec::new());
    let on_progress = |p: ListingProgress| seen.lock().unwrap().push(p.dirs);

    let listing = async {
        // Nothing yet for the first tick, then a count that climbs. The sleeps ARE the
        // slow listing, on paused time, so they cost nothing.
        // allowed-test-sleep: simulated listing latency under start_paused
        tokio::time::sleep(Duration::from_millis(250)).await;
        listed.store(10, Ordering::Relaxed);
        // allowed-test-sleep: simulated listing latency under start_paused
        tokio::time::sleep(Duration::from_millis(200)).await;
        listed.store(20, Ordering::Relaxed);
        // allowed-test-sleep: simulated listing latency under start_paused
        tokio::time::sleep(Duration::from_millis(200)).await;
        "done"
    };
    let answer = report_while(
        listing,
        Duration::from_millis(200),
        || ListingProgress {
            files: 0,
            dirs: listed.load(Ordering::Relaxed),
            bytes: 0,
            ..ListingProgress::default()
        },
        Some(&on_progress),
    )
    .await;

    assert_eq!(answer, "done");
    assert_eq!(
        *seen.lock().unwrap(),
        [10, 20],
        "a zero count would render as 'Loaded 0 items'"
    );
}

#[tokio::test]
async fn with_nobody_listening_it_only_awaits_the_listing() {
    let samples = AtomicUsize::new(0);
    let answer = report_while(
        async { 7 },
        Duration::from_millis(1),
        || {
            samples.fetch_add(1, Ordering::Relaxed);
            ListingProgress::default()
        },
        None,
    )
    .await;

    assert_eq!(answer, 7);
    assert_eq!(samples.load(Ordering::Relaxed), 0);
}
