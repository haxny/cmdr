//! When the lifecycle observers and the FSEvents watch hear about a listing,
//! relative to the cache.
//!
//! - An observer hears an open AFTER the cache insert, and a close AFTER the
//!   cache removal: an observer's detached arm reconciles against cache
//!   membership, so a close it heard early would let a racing arm re-take what
//!   the close just gave back.
//! - A listing on a watchable volume arms its FSEvents watch in the TEST binary
//!   too, with no registration at all. Tests lean on that
//!   (`local_posix_test::test_listing_watch_coverage_flips_with_watcher_lifecycle`,
//!   `caching_reaper_test`, `archive_watch_integration_test`), so a move that
//!   arms only in the app would leave them passing for the wrong reason.
//!
//! A detached arm that lands after its listing ended dropping its watch is
//! `watcher_test::a_detached_arm_that_lost_the_race_leaves_no_watch_behind`.

use std::path::Path;
use std::sync::Arc;

use super::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use super::operations::list_directory_end;
use super::seam_test_support::{Heard, heard, record_lifecycle_of};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use super::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::volume::{InMemoryVolume, Volume};

async fn stream(listing_id: &str, volume_id: &str, path: &Path) {
    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());
    read_directory_with_progress(
        &events,
        listing_id,
        &Arc::new(StreamingListingState::new()),
        volume_id,
        path,
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await
    .expect("the volume lists");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_observer_hears_an_open_after_the_listing_is_cached() {
    let volume_id = unique_test_id("lifecycle-open-vol");
    get_volume_manager().register(&volume_id, Arc::new(InMemoryVolume::new("Open order")));
    let listing = TestListingGuard::adopt(unique_test_id("lifecycle-open"));
    record_lifecycle_of(listing.id());

    stream(listing.id(), &volume_id, Path::new("/")).await;

    assert_eq!(heard(listing.id()), [Heard::Opened { cached: true }]);
    get_volume_manager().unregister(&volume_id);
}

#[test]
fn an_observer_hears_a_close_after_the_listing_left_the_cache() {
    let listing = TestListing::new()
        .volume(&unique_test_id("lifecycle-close-vol"))
        .insert("lifecycle-close");
    record_lifecycle_of(listing.id());

    list_directory_end(listing.id());

    assert_eq!(heard(listing.id()), [Heard::Closed { cached: false }]);
}

/// Whether a watch is registered for `listing_id`.
#[cfg(target_os = "macos")]
fn is_watching(listing_id: &str) -> bool {
    use crate::file_system::watcher::WATCHER_MANAGER;
    use cmdr_fs::ignore_poison::RwLockIgnorePoison;

    WATCHER_MANAGER.read_ignore_poison().watches.contains_key(listing_id)
}

/// A real folder on a `LocalPosixVolume`, registered under a fresh id.
#[cfg(target_os = "macos")]
fn local_volume(tag: &str) -> (crate::test_support::TestDir, String) {
    use crate::file_system::volume::LocalPosixVolume;

    let scratch = crate::test_support::TestDir::new(tag);
    std::fs::write(scratch.join("a.txt"), b"a").expect("scratch dir is writable");
    let volume_id = unique_test_id(&format!("{tag}-vol"));
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("Arming", scratch.to_path_buf()));
    assert!(volume.can_watch_listings(), "the fixture must be watchable");
    get_volume_manager().register(&volume_id, volume);
    (scratch, volume_id)
}

/// Generous: the arm runs on the blocking pool, which a saturated run starves.
#[cfg(target_os = "macos")]
const ARMS_WITHIN: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_streamed_listing_arms_its_watch_in_the_test_binary() {
    use crate::test_support::wait_until_async;

    let (scratch, volume_id) = local_volume("arming-streamed");
    let listing = TestListingGuard::adopt(unique_test_id("arming-streamed"));

    stream(listing.id(), &volume_id, &scratch).await;

    wait_until_async(ARMS_WITHIN, "the streamed listing to arm its watch", || {
        is_watching(listing.id())
    })
    .await;
    drop(listing);
    get_volume_manager().unregister(&volume_id);
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_synchronously_started_listing_arms_its_watch_in_the_test_binary() {
    use crate::test_support::wait_until_async;

    let (scratch, volume_id) = local_volume("arming-sync");

    let started = super::operations::list_directory_start_with_volume(
        &volume_id,
        &scratch,
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await
    .expect("the folder lists");
    let listing = TestListingGuard::adopt(started.listing_id);

    wait_until_async(ARMS_WITHIN, "the started listing to arm its watch", || {
        is_watching(listing.id())
    })
    .await;
    drop(listing);
    get_volume_manager().unregister(&volume_id);
}
