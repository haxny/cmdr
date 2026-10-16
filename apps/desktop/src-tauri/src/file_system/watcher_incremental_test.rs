//! What the watcher's two halves do to a listing, pinned before issue #301 lifts
//! them into the listing engine (`observe_children` and `refresh`).
//!
//! - The incremental path classifies a debounced batch against the cache and
//!   publishes removes, then adds, then modifies; a modify keeps the Finder tags
//!   the cache already holds.
//! - Over 500 events, or any `Any` / `Other` kind, it gives up on classifying and
//!   re-reads the whole directory instead.
//! - `handle_directory_change` returns only once its re-read LANDED in the cache.
//!   `flush_all_watchers` (E2E's `flush_file_watcher`) awaits it per watch and
//!   relies on exactly that; the flush itself is `playwright-e2e`-only, so it
//!   can't run in this binary.
//!
//! Every cell uses an unregistered volume id, so the re-read takes the `std::fs`
//! fallback and nothing here depends on the shared volume registry.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use notify_debouncer_full::DebouncedEvent;
use notify_debouncer_full::notify::Event;
use notify_debouncer_full::notify::event::{DataChange, EventKind, ModifyKind};

use super::watcher::{handle_directory_change, handle_directory_change_incremental};
use crate::test_support::{TestDir, wait_until};
use cmdr_fs::entry::TagRef;
use cmdr_fs::testing::unique_test_id;
use cmdr_listing::DiffChangeType;
use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_listing::testing::{hold_diffs, pending_changes};
use cmdr_local::reading::list_directory_core;

fn event(kind: EventKind, path: &Path) -> DebouncedEvent {
    DebouncedEvent::new(
        Event {
            kind,
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        },
        Instant::now(),
    )
}

fn content_change(path: &Path) -> DebouncedEvent {
    event(EventKind::Modify(ModifyKind::Data(DataChange::Content)), path)
}

/// A pane's listing of `dir` as `std::fs` sees it now, on an unregistered volume.
fn listing_of(dir: &Path, tag: &str) -> TestListingGuard {
    TestListing::new()
        .volume(&unique_test_id(&format!("{tag}-vol")))
        .path(dir.to_path_buf())
        .entries(list_directory_core(dir, &crate::listing_host::backends::AppListings).expect("the scratch dir lists"))
        .insert(crate::listing_host::listings(), tag)
}

#[test]
fn a_batch_publishes_removes_then_adds_then_modifies() {
    let scratch = TestDir::new("watch-batch-order");
    let removed = scratch.join("removed.txt");
    let modified = scratch.join("modified.txt");
    let added = scratch.join("added.txt");
    std::fs::write(&removed, b"r").expect("scratch dir is writable");
    std::fs::write(&modified, b"m").expect("scratch dir is writable");
    let listing = listing_of(&scratch, "watch-batch-order");
    hold_diffs(listing.engine(), listing.id());

    std::fs::remove_file(&removed).expect("scratch dir is writable");
    std::fs::write(&modified, b"modified").expect("scratch dir is writable");
    std::fs::write(&added, b"a").expect("scratch dir is writable");
    // One batch naming all three, in the order least likely to come out right.
    handle_directory_change_incremental(
        listing.id(),
        vec![
            content_change(&modified),
            content_change(&added),
            content_change(&removed),
        ],
    );

    let queued: Vec<(DiffChangeType, String)> = pending_changes(listing.engine(), listing.id())
        .into_iter()
        .map(|c| (c.change_type, c.entry.name))
        .collect();
    assert_eq!(
        queued,
        [
            (DiffChangeType::Remove, "removed.txt".to_string()),
            (DiffChangeType::Add, "added.txt".to_string()),
            (DiffChangeType::Modify, "modified.txt".to_string()),
        ]
    );
}

#[test]
fn a_modify_keeps_the_finder_tags_the_cache_holds() {
    let scratch = TestDir::new("watch-modify-tags");
    let tagged = scratch.join("tagged.txt");
    std::fs::write(&tagged, b"t").expect("scratch dir is writable");
    let mut entries =
        list_directory_core(&scratch, &crate::listing_host::backends::AppListings).expect("the scratch dir lists");
    entries[0].tags = vec![TagRef {
        name: "Red".to_string(),
        color: 6,
    }];
    let listing = TestListing::new()
        .volume(&unique_test_id("watch-modify-tags-vol"))
        .path(scratch.to_path_buf())
        .entries(entries)
        .insert(crate::listing_host::listings(), "watch-modify-tags");

    std::fs::write(&tagged, b"touched").expect("scratch dir is writable");
    handle_directory_change_incremental(listing.id(), vec![content_change(&tagged)]);

    let entry = listing.entries().into_iter().next().expect("the file is still listed");
    assert_eq!(entry.size, Some(7), "the modify landed");
    assert_eq!(
        entry.tags.len(),
        1,
        "the re-stat reads no xattr, so the cached tags carry forward"
    );
}

/// A folder holding `seen.txt`, listed, then given `unseen.txt` behind the
/// cache's back. Only a full re-read finds `unseen.txt`: no event names it.
fn listing_missing_a_file(tag: &str) -> (TestDir, PathBuf, TestListingGuard) {
    let scratch = TestDir::new(tag);
    let seen = scratch.join("seen.txt");
    std::fs::write(&seen, b"s").expect("scratch dir is writable");
    let listing = listing_of(&scratch, tag);
    std::fs::write(scratch.join("unseen.txt"), b"u").expect("scratch dir is writable");
    (scratch, seen, listing)
}

/// Generous: the fallback re-read runs on a spawned task, which a saturated run starves.
const RE_READ_LANDS_WITHIN: Duration = Duration::from_secs(30);

fn wait_for_the_full_re_read(listing: &TestListingGuard) {
    wait_until(RE_READ_LANDS_WITHIN, "the fallback full re-read to land", || {
        listing.entry_names().iter().any(|n| n == "unseen.txt")
    });
}

#[test]
fn over_500_events_fall_back_to_a_full_re_read() {
    let (_scratch, seen, listing) = listing_missing_a_file("watch-over-500");

    handle_directory_change_incremental(listing.id(), (0..501).map(|_| content_change(&seen)).collect());

    wait_for_the_full_re_read(&listing);
}

#[test]
fn an_any_kind_falls_back_to_a_full_re_read() {
    let (_scratch, seen, listing) = listing_missing_a_file("watch-any-kind");

    handle_directory_change_incremental(listing.id(), vec![event(EventKind::Any, &seen)]);

    wait_for_the_full_re_read(&listing);
}

#[test]
fn an_other_kind_falls_back_to_a_full_re_read() {
    let (_scratch, seen, listing) = listing_missing_a_file("watch-other-kind");

    handle_directory_change_incremental(listing.id(), vec![event(EventKind::Other, &seen)]);

    wait_for_the_full_re_read(&listing);
}

/// No wait after the await: the re-read must already be in the cache.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_re_read_has_landed_when_it_returns() {
    let (_scratch, _seen, listing) = listing_missing_a_file("watch-re-read-lands");

    handle_directory_change(listing.id()).await;

    assert_eq!(listing.entry_names(), ["seen.txt", "unseen.txt"]);
}
