//! Overlay rows survive EVERY read path that builds or replaces a listing:
//! the streaming read, a watcher's `FullRefresh`, a backend swap's respell
//! re-read, the watcher's own full re-read (`handle_directory_change`, also ⌘R),
//! and the test-only synchronous start. Each one re-reads through the volume,
//! which holds no contributed row, so a path that skips the overlays strips
//! them from an open pane.
//!
//! A device backend's `Replaced` hand-over is the one replacement that records
//! zero contributed rows. Its arm in `notify_directory_changed` sits behind the
//! `has_app` gate, which a test binary never passes, so the cell drives its
//! per-listing half, as `caching_replacement_test.rs` does.
//!
//! The overlay is `seam_test_support`'s marker, which decorates only a volume
//! with its name. What the git portal decides is `file_system/git/overlay_tests.rs`.

use std::path::Path;
use std::sync::Arc;

use super::caching::{notify_full_refresh, publish_replacement};
use super::caching_test_support::{TestListing, TestListingGuard, unique_test_id};
use super::metadata::FileEntry;
use super::operations::list_directory_start_with_volume;
use super::seam_test_support::{MARKER_ROW, MARKER_VOLUME_NAME, ensure_seam_overlay};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use super::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::volume::manager::get_volume_manager;

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), false, false)
}

/// A marker volume holding `a.txt` and `b.txt` at its root, registered under a
/// fresh id that the guard unregisters.
struct MarkerVolume(String);

impl MarkerVolume {
    fn new(tag: &str) -> Self {
        ensure_seam_overlay();
        let id = unique_test_id(tag);
        get_volume_manager().register(
            &id,
            Arc::new(InMemoryVolume::with_entries(
                MARKER_VOLUME_NAME,
                vec![entry("a.txt"), entry("b.txt")],
            )),
        );
        Self(id)
    }

    /// A pane's listing of the root as an earlier read left it, holding `a.txt`
    /// only and no contributed row, so a re-read has something to change.
    fn stale_listing(&self, tag: &str) -> TestListingGuard {
        TestListing::new()
            .volume(&self.0)
            .path("/")
            .entries(vec![entry("a.txt")])
            .insert(tag)
    }
}

impl Drop for MarkerVolume {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.0);
    }
}

/// The re-read landed (`b.txt` arrived) and carries the marker row, counted.
fn assert_decorated(listing: &TestListingGuard) {
    let names = listing.entry_names();
    assert!(names.iter().any(|n| n == "b.txt"), "the re-read landed: {names:?}");
    assert!(
        names.iter().any(|n| n == MARKER_ROW),
        "the overlay row survived: {names:?}"
    );
    assert!(
        listing.with_listing(|l| l.has_overlay_rows()),
        "the contributed row is counted, so the oracle keeps declining this listing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_streaming_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-streaming");
    let listing = TestListingGuard::adopt(unique_test_id("overlay-path-streaming"));
    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());

    read_directory_with_progress(
        &events,
        listing.id(),
        &Arc::new(StreamingListingState::new()),
        &volume.0,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await
    .expect("the marker volume lists");

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_refresh_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-full-refresh");
    let listing = volume.stale_listing("overlay-path-full-refresh");

    notify_full_refresh(
        volume.0.clone(),
        "/".into(),
        vec![(
            listing.id().to_string(),
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        )],
    )
    .await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_respell_re_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-respell");
    let listing = volume.stale_listing("overlay-path-respell");

    super::foreign_path::respell_listing(volume.0.clone(), listing.id().to_string(), "/".into()).await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_watchers_full_re_read_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-watcher");
    let listing = volume.stale_listing("overlay-path-watcher");

    crate::file_system::watcher::handle_directory_change(listing.id()).await;

    assert_decorated(&listing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_synchronous_start_folds_in_overlay_rows() {
    let volume = MarkerVolume::new("overlay-path-sync-start");

    let started = list_directory_start_with_volume(
        &volume.0,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await
    .expect("the marker volume lists");
    let listing = TestListingGuard::adopt(started.listing_id);

    assert_decorated(&listing);
}

/// The `Replaced` half: a backend's own re-read becomes the listing with zero
/// contributed rows, even on a listing an earlier read decorated.
#[test]
fn a_replaced_hand_over_records_zero_overlay_rows() {
    let listing = TestListing::new()
        .volume(&unique_test_id("overlay-path-replaced-vol"))
        .path("/")
        .entries(vec![entry("a.txt"), entry(MARKER_ROW)])
        .overlay_rows(1)
        .insert("overlay-path-replaced");

    publish_replacement(listing.id(), vec![entry("a.txt"), entry("b.txt")], 0);

    assert_eq!(listing.entry_names(), ["a.txt", "b.txt"]);
    assert!(
        !listing.with_listing(|l| l.has_overlay_rows()),
        "a backend's hand-over counts no contributed row"
    );
}
