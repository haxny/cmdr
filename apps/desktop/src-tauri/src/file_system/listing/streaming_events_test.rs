//! A streaming listing end to end, as the `ListingEvents` seam hears it: the
//! spawned task on the host runtime, the read, and the one `finished` report
//! per listing, whichever way it ends.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::caching_test_support::{SpelledVolume, TestListingGuard};
use super::host::ListingEvents;
use super::host::testing::RecordingEvents;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use super::streaming::{ListingRequest, start_streaming_listing};
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::wait_until_async;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::unique_test_id;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

/// Registers `volume` under a fresh id; dropping it unregisters.
struct Registered(String);

impl Registered {
    fn new(tag: &str, volume: Arc<dyn Volume>) -> Self {
        let id = unique_test_id(tag);
        get_volume_manager().register(&id, volume);
        Self(id)
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.0);
    }
}

/// Starts a listing of `/` on `volume_id` the way the IPC command does, and
/// waits for its last report.
async fn list_root(volume_id: &str, listing: &TestListingGuard) -> Arc<RecordingEvents> {
    let recorded = Arc::new(RecordingEvents::new());
    start_streaming_listing(
        Arc::clone(&recorded) as Arc<dyn ListingEvents>,
        ListingRequest {
            id: super::listing_id::ListingId::new(listing.id()),
            volume_id: volume_id.to_string(),
            path: Path::new("/").to_path_buf(),
            include_hidden: true,
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            dir_sort_mode: DirectorySortMode::LikeFiles,
        },
    );
    wait_until_async(Duration::from_secs(10), "the listing's last report", || {
        recorded
            .sequence()
            .last()
            .is_some_and(|kind| ["complete", "cancelled", "failed"].contains(kind))
    })
    .await;
    recorded
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landed_listing_reports_opening_then_its_read_then_complete() {
    let volume = Registered::new(
        "events-ok",
        Arc::new(InMemoryVolume::with_entries(
            "Ok",
            vec![FileEntry::new("a.txt".to_string(), "/a.txt".to_string(), false, false)],
        )),
    );
    let listing = TestListingGuard::adopt(unique_test_id("events-ok"));

    let recorded = list_root(&volume.0, &listing).await;

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
    let volume = Registered::new(
        "events-refused",
        Arc::new(SpelledVolume::new(InMemoryVolume::new("Refusing")).listings_refused_with(VolumeError::NotSupported)),
    );
    let listing = TestListingGuard::adopt(unique_test_id("events-refused"));

    let recorded = list_root(&volume.0, &listing).await;

    assert_eq!(recorded.sequence(), ["opening", "failed"]);
    assert!(!listing.is_cached());
}
