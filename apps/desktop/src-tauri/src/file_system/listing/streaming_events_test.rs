//! A streaming listing end to end, as the `ListingEvents` seam hears it: the
//! spawned task on the host runtime, the read, and the one `finished` report
//! per listing, whichever way it ends.

use std::sync::Arc;
use std::time::Duration;

use super::testing::{SpelledVolume, TestEngine, TestListingGuard, request};
use crate::test_support::wait_until_async;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

/// An engine serving `volume` under `volume_id`.
fn engine_with(volume_id: &str, volume: Arc<dyn Volume>) -> TestEngine {
    TestEngine::builder().volume(volume_id, volume).build()
}

/// Starts a listing of `/` on `volume_id` the way the IPC command does, and
/// waits for its last report.
async fn list_root(engine: &TestEngine, volume_id: &str, listing: &TestListingGuard) {
    engine.start(request(listing.id(), volume_id, "/"));
    wait_until_async(Duration::from_secs(10), "the listing's last report", || {
        engine
            .events
            .sequence()
            .last()
            .is_some_and(|kind| ["complete", "cancelled", "failed"].contains(kind))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landed_listing_reports_opening_then_its_read_then_complete() {
    let engine = engine_with(
        "events-ok",
        Arc::new(InMemoryVolume::with_entries(
            "Ok",
            vec![FileEntry::new("a.txt".to_string(), "/a.txt".to_string(), false, false)],
        )),
    );
    let listing = TestListingGuard::adopt(&engine, unique_test_id("events-ok"));

    list_root(&engine, "events-ok", &listing).await;

    let recorded = &engine.events;
    assert_eq!(recorded.sequence(), ["opening", "read_complete", "complete"]);
    assert_eq!(
        recorded.complete.lock_ignore_poison().as_slice(),
        [(listing.id().to_string(), 1)]
    );
    assert!(listing.is_cached());
}

/// The one report the read doesn't make itself: a refused read is reported by
/// its task, after the cancel token is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_listing_reports_opening_then_failed() {
    let engine = engine_with(
        "events-refused",
        Arc::new(SpelledVolume::new(InMemoryVolume::new("Refusing")).listings_refused_with(VolumeError::NotSupported)),
    );
    let listing = TestListingGuard::adopt(&engine, unique_test_id("events-refused"));

    list_root(&engine, "events-refused", &listing).await;

    assert_eq!(engine.events.sequence(), ["opening", "failed"]);
    assert!(!listing.is_cached());
}
