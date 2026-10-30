//! The link-watch bookkeeping, server-free: holds count per link path, the
//! last one stops the watch, a watch that gave up restarts on the next hold,
//! and only a live watch covers a path. The watch itself against a real link:
//! `dfs_links_integration_test.rs`.

use super::*;

fn live(watches: &LinkWatches, root: &str, live: bool) {
    watches.links.lock_ignore_poison()[root]
        .status
        .live
        .store(live, Ordering::Release);
}

fn holds(watches: &LinkWatches, root: &str) -> Option<usize> {
    watches.links.lock_ignore_poison().get(root).map(|watch| watch.holds)
}

#[test]
fn the_first_hold_starts_a_watch_and_the_others_share_it() {
    let watches = Arc::new(LinkWatches::default());
    let (first, start) = watches.hold("data");
    assert!(start.is_some(), "the link's first listing starts its watch");
    let (second, start) = watches.hold("data");
    assert!(start.is_none(), "a second listing behind it shares the watch");
    assert_eq!(holds(&watches, "data"), Some(2));

    drop(first);
    assert_eq!(holds(&watches, "data"), Some(1), "one listing still holds it");
    drop(second);
    assert_eq!(holds(&watches, "data"), None, "the last listing stops it");
}

#[test]
fn the_last_hold_cancels_the_task() {
    let watches = Arc::new(LinkWatches::default());
    let (hold, start) = watches.hold("data");
    let mut cancel_rx = start.unwrap().cancel_rx;
    assert!(
        cancel_rx
            .try_recv()
            .is_err_and(|e| e == oneshot::error::TryRecvError::Empty)
    );
    drop(hold);
    assert_eq!(
        cancel_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Closed),
        "the task hears the cancel"
    );
}

#[test]
fn a_watch_that_gave_up_restarts_on_the_next_hold() {
    let watches = Arc::new(LinkWatches::default());
    let (first, start) = watches.hold("data");
    let status = start.unwrap().status;
    drop(Running(status)); // the task ends: backoff exhausted

    let (second, start) = watches.hold("data");
    assert!(
        start.is_some(),
        "a listing opening behind a dead watch starts a new one"
    );
    assert_eq!(
        holds(&watches, "data"),
        Some(2),
        "the earlier listing's hold carries over"
    );
    drop((first, second));
    assert_eq!(holds(&watches, "data"), None);
}

#[test]
fn only_a_live_watch_covers_a_path() {
    let watches = Arc::new(LinkWatches::default());
    let (_hold, _start) = watches.hold("data/sub");
    assert!(!watches.is_live_for("data/sub"), "arming isn't watching yet");

    live(&watches, "data/sub", true);
    assert!(watches.is_live_for("data/sub"));
    assert!(watches.is_live_for("data/sub/deeper"), "below the link path too");
    assert!(!watches.is_live_for("data"), "not above it");
    assert!(!watches.is_live_for("data/sub2"), "nor a sibling sharing a prefix");

    live(&watches, "data/sub", false);
    assert!(!watches.is_live_for("data/sub"), "a lost watch covers nothing");
}

#[test]
fn stopping_all_leaves_late_releases_harmless() {
    let watches = Arc::new(LinkWatches::default());
    let (hold, _start) = watches.hold("data");
    live(&watches, "data", true);
    watches.stop_all();
    assert!(!watches.is_live_for("data"));
    drop(hold);
    let (_again, start) = watches.hold("data");
    assert!(start.is_some(), "a fresh hold after a stop starts afresh");
}
