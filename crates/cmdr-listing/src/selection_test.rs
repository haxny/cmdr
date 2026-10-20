//! Reading the pane's selection back: the selected rows only mean something
//! against the revision the pane applied them in.

use super::operations::ListingLookupError;
use super::testing::TestEngine;
use super::testing::{TestListing, TestListingGuard};
use cmdr_fs::entry::FileEntry;

const DIR: &str = "/test/selection";

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
}

fn pane(tag: &str) -> TestListingGuard {
    TestListing::new()
        .path(DIR)
        .include_hidden(false)
        .entries(vec![entry("alpha.txt"), entry("charlie.txt")])
        .insert(&TestEngine::new(), tag)
}

#[test]
fn a_watcher_insert_above_the_selection_refuses_the_entries_of_the_old_rows() {
    let listing = pane("selection-entries-insert");
    // The pane applied revision 0 and selected row 1, `charlie.txt`. A file lands
    // above it before the pane hears the diff, so row 1 is now `bravo.txt`.
    listing.engine().inner.notify_added(listing.id(), entry("bravo.txt"));

    assert!(matches!(
        listing
            .engine()
            .selection_entries(&listing.listing_id(), false, &[1], 0),
        Err(ListingLookupError::Changed { .. })
    ));
    let names: Vec<String> = listing
        .engine()
        .selection_entries(&listing.listing_id(), false, &[2], 1)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, vec!["charlie.txt".to_string()]);
}

#[test]
fn the_entries_of_a_selection_row_past_the_end_are_refused() {
    let listing = pane("selection-entries-past-end");
    assert!(matches!(
        listing
            .engine()
            .selection_entries(&listing.listing_id(), false, &[2], 0),
        Err(ListingLookupError::Changed { .. })
    ));
}

#[test]
fn the_entries_read_under_the_other_hidden_files_setting_are_refused() {
    let listing = pane("selection-entries-hidden");
    assert!(matches!(
        listing.engine().selection_entries(&listing.listing_id(), true, &[0], 0),
        Err(ListingLookupError::Changed { .. })
    ));
}
