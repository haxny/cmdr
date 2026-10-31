//! `ScanListingClock`: paused time never counts, running time always does.

use super::*;

#[tokio::test(start_paused = true)]
async fn a_running_clock_counts_wall_time() {
    let clock = ScanListingClock::start();
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(clock.active(), Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn paused_time_never_counts_even_while_the_pause_is_open() {
    let clock = ScanListingClock::start();
    tokio::time::advance(Duration::from_secs(2)).await;
    let pause = clock.pause();
    tokio::time::advance(Duration::from_secs(600)).await;
    assert_eq!(clock.active(), Duration::from_secs(2), "an open pause must not leak in");
    drop(pause);
    tokio::time::advance(Duration::from_secs(3)).await;
    assert_eq!(clock.active(), Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn pauses_nest_and_the_clock_resumes_only_after_the_last() {
    let clock = ScanListingClock::start();
    let outer = clock.pause();
    let inner = clock.clone().pause();
    tokio::time::advance(Duration::from_secs(10)).await;
    drop(outer);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(clock.active(), Duration::ZERO, "one pause still holds it");
    drop(inner);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(clock.active(), Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn paused_wraps_a_wait_and_resumes_after_it() {
    let clock = ScanListingClock::start();
    // allowed-test-sleep: the paused wait IS the subject, on a paused runtime.
    clock.paused(tokio::time::sleep(Duration::from_secs(300))).await;
    tokio::time::advance(Duration::from_secs(4)).await;
    assert_eq!(clock.active(), Duration::from_secs(4));
}
