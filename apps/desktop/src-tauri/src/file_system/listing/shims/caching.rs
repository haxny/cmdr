//! Shims over the installed engine for `caching`'s callers (`shims/mod.rs`).

use std::path::{Path, PathBuf};

use cmdr_fs::entry::{FileEntry, TagRef};
use cmdr_fs::volume::DirectoryChange;

use super::installed;
use crate::file_system::listing::diff::PaneRows;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};

/// `Inner::find_listings_for_path_on_volume` on the installed engine.
// shim: removed in M4b
pub fn find_listings_for_path_on_volume(
    volume_id: Option<&str>,
    parent_path: &Path,
) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    installed().find_listings_for_path_on_volume(volume_id, parent_path)
}

/// `Inner::find_listings_on_volume` on the installed engine.
// shim: removed in M4b
pub(crate) fn find_listings_on_volume(
    volume_id: &str,
) -> Vec<(String, PathBuf, SortColumn, SortOrder, DirectorySortMode)> {
    installed().find_listings_on_volume(volume_id)
}

/// `Inner::insert_entry_sorted` on the installed engine.
// shim: removed in M4b
pub fn insert_entry_sorted(listing_id: &str, entry: FileEntry) -> Option<PaneRows> {
    installed().insert_entry_sorted(listing_id, entry)
}

/// `Inner::remove_entries_by_paths` on the installed engine.
// shim: removed in M4b
pub fn remove_entries_by_paths(listing_id: &str, paths: &[PathBuf]) -> Vec<(PaneRows, FileEntry)> {
    installed().remove_entries_by_paths(listing_id, paths)
}

/// `Inner::remove_entry_by_name` on the installed engine.
// shim: removed in M4b
pub fn remove_entry_by_name(listing_id: &str, name: &std::ffi::OsStr) -> Option<(PaneRows, FileEntry)> {
    installed().remove_entry_by_name(listing_id, name)
}

/// `Inner::has_entry` on the installed engine.
// shim: removed in M4b
pub fn has_entry(listing_id: &str, path: &str) -> bool {
    installed().has_entry(listing_id, path)
}

/// `Inner::update_entry_sorted` on the installed engine.
// shim: removed in M4b
pub fn update_entry_sorted(listing_id: &str, new_entry: FileEntry) -> Option<PaneRows> {
    installed().update_entry_sorted(listing_id, new_entry)
}

/// `Inner::carry_forward_tags` on the installed engine.
// shim: removed in M4b
pub fn carry_forward_tags(listing_id: &str, entry: &mut FileEntry) {
    installed().carry_forward_tags(listing_id, entry);
}

/// `Inner::apply_tags_to_listing` on the installed engine.
// shim: removed in M4b
pub fn apply_tags_to_listing(listing_id: &str, updates: Vec<(String, Vec<TagRef>)>) {
    installed().apply_tags_to_listing(listing_id, updates);
}

/// `Inner::notify_directory_changed` on the installed engine.
// shim: removed in M4b
pub fn notify_directory_changed(volume_id: &str, parent_path: &Path, change: DirectoryChange) {
    installed().notify_directory_changed(volume_id, parent_path, change);
}

/// `Inner::notify_added` on the installed engine.
// shim: removed in M4b
pub(crate) fn notify_added(listing_id: &str, entry: FileEntry) {
    installed().notify_added(listing_id, entry);
}

/// `Inner::notify_removed` on the installed engine.
// shim: removed in M4b
pub(crate) fn notify_removed(listing_id: &str, full_path: &Path) {
    installed().notify_removed(listing_id, full_path);
}

/// `Inner::spawn_full_refresh` on the installed engine.
// shim: removed in M4b
pub(crate) fn spawn_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    installed().spawn_full_refresh(volume_id, parent_path, listings);
}

/// `Inner::notify_full_refresh` on the installed engine.
// shim: removed in M4b
pub(crate) async fn notify_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    installed().notify_full_refresh(volume_id, parent_path, listings).await;
}

/// `Inner::publish_replacement` on the installed engine.
// shim: removed in M4b
pub(crate) fn publish_replacement(listing_id: &str, entries: Vec<FileEntry>, overlay_rows: usize) {
    use crate::file_system::listing::pipeline::{OverlayRows, Prepared};
    // A re-read starting now, as a test's hand-over always is.
    installed().publish_replacement(
        listing_id,
        Prepared::for_test(entries, OverlayRows::Recounted(overlay_rows)),
        installed().next_read_generation(),
    );
}

/// `Inner::try_get_authoritative_listing` on the installed engine.
// shim: removed in M4b
pub fn try_get_authoritative_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
    installed().try_get_authoritative_listing(volume_id, path)
}
