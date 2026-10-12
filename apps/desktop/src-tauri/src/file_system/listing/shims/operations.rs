//! Shims over the installed engine for `operations`'s callers (`shims/mod.rs`).

use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use cmdr_fs::entry::FileEntry;

use super::installed;
use crate::file_system::listing::cached_listing::OverlayRows;
#[cfg(test)]
use crate::file_system::listing::operations::ListingStartResult;
use crate::file_system::listing::operations::{
    ListingLookupError, ListingStats, ResortResult, RowBeside, SelectionSnapshot,
};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};

/// `Inner::list_directory_start_with_volume` on the installed engine.
// shim: removed in M4b
#[cfg(test)]
pub async fn list_directory_start_with_volume(
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> Result<ListingStartResult, std::io::Error> {
    installed()
        .list_directory_start_with_volume(volume_id, path, include_hidden, sort_by, sort_order, dir_sort_mode)
        .await
}

/// `Inner::list_directory_end` on the installed engine.
// shim: removed in M4b
pub fn list_directory_end(listing_id: &str) {
    installed().list_directory_end(listing_id);
}

/// `Inner::set_listing_include_hidden` on the installed engine.
// shim: removed in M4b
pub fn set_listing_include_hidden(
    listing_id: &str,
    include_hidden: bool,
    expected_sequence: Option<u64>,
    cursor_filename: Option<&str>,
    selected_indices: Option<&[usize]>,
    all_selected: bool,
) -> Result<ResortResult, ListingLookupError> {
    installed().set_listing_include_hidden(
        listing_id,
        include_hidden,
        expected_sequence,
        cursor_filename,
        selected_indices,
        all_selected,
    )
}

/// `Inner::keep_listings_alive` on the installed engine.
// shim: removed in M4b
pub fn keep_listings_alive(listing_ids: &[String]) -> Vec<String> {
    installed().keep_listings_alive(listing_ids)
}

/// `Inner::get_file_range` on the installed engine.
// shim: removed in M4b
pub fn get_file_range(
    listing_id: &str,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, ListingLookupError> {
    installed().get_file_range(listing_id, start, count, include_hidden)
}

/// `Inner::get_total_count` on the installed engine.
// shim: removed in M4b
#[cfg(test)]
pub fn get_total_count(listing_id: &str, include_hidden: bool) -> Result<usize, ListingLookupError> {
    installed().get_total_count(listing_id, include_hidden)
}

/// `Inner::find_file_index` on the installed engine.
// shim: removed in M4b
pub fn find_file_index(
    listing_id: &str,
    name: &str,
    include_hidden: bool,
) -> Result<Option<usize>, ListingLookupError> {
    installed().find_file_index(listing_id, name, include_hidden)
}

/// `Inner::get_file_beside` on the installed engine.
// shim: removed in M4b
pub fn get_file_beside(
    listing_id: &str,
    name: &str,
    side: RowBeside,
    include_hidden: bool,
) -> Result<Option<FileEntry>, ListingLookupError> {
    installed().get_file_beside(listing_id, name, side, include_hidden)
}

/// `Inner::find_file_indices` on the installed engine.
// shim: removed in M4b
pub fn find_file_indices(
    listing_id: &str,
    names: &[String],
    include_hidden: bool,
) -> Result<HashMap<String, usize>, ListingLookupError> {
    installed().find_file_indices(listing_id, names, include_hidden)
}

/// `Inner::get_file_at` on the installed engine.
// shim: removed in M4b
pub fn get_file_at(
    listing_id: &str,
    index: usize,
    include_hidden: bool,
) -> Result<Option<FileEntry>, ListingLookupError> {
    installed().get_file_at(listing_id, index, include_hidden)
}

/// `Inner::get_paths_at_indices` on the installed engine.
// shim: removed in M4b
pub fn get_paths_at_indices(
    listing_id: &str,
    selected_indices: &[usize],
    include_hidden: bool,
    has_parent: bool,
) -> Result<Vec<PathBuf>, ListingLookupError> {
    installed().get_paths_at_indices(listing_id, selected_indices, include_hidden, has_parent)
}

/// `Inner::get_files_at_indices` on the installed engine.
// shim: removed in M4b
pub fn get_files_at_indices(
    listing_id: &str,
    selected_indices: &[usize],
    include_hidden: bool,
) -> Result<Vec<FileEntry>, ListingLookupError> {
    installed().get_files_at_indices(listing_id, selected_indices, include_hidden)
}

/// `Inner::resort_listing` on the installed engine.
// shim: removed in M4b
#[allow(
    clippy::too_many_arguments,
    reason = "Resort requires sort params, cursor tracking, and selection state"
)]
pub fn resort_listing(
    listing_id: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    cursor_filename: Option<&str>,
    include_hidden: bool,
    selected_indices: Option<&[usize]>,
    all_selected: bool,
    expected_sequence: Option<u64>,
) -> Result<ResortResult, ListingLookupError> {
    installed().resort_listing(
        listing_id,
        sort_by,
        sort_order,
        dir_sort_mode,
        cursor_filename,
        include_hidden,
        selected_indices,
        all_selected,
        expected_sequence,
    )
}

/// `Inner::get_listing_entries` on the installed engine.
// shim: removed in M4b
pub(crate) fn get_listing_entries(listing_id: &str) -> Option<(PathBuf, Vec<FileEntry>)> {
    installed().get_listing_entries(listing_id)
}

/// `Inner::update_listing_entries` on the installed engine.
// shim: removed in M4b
pub(crate) fn update_listing_entries(listing_id: &str, entries: Vec<FileEntry>, overlay_rows: OverlayRows) {
    installed().update_listing_entries(listing_id, entries, overlay_rows);
}

/// `Inner::get_selection_snapshot` on the installed engine.
// shim: removed in M4b
pub fn get_selection_snapshot(
    listing_id: &str,
    include_hidden: bool,
    selected_indices: &[usize],
    expected_sequence: u64,
) -> Result<SelectionSnapshot, ListingLookupError> {
    installed().get_selection_snapshot(listing_id, include_hidden, selected_indices, expected_sequence)
}

/// `Inner::volume_ids_with_listings` on the installed engine.
// shim: removed in M4b
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn volume_ids_with_listings(prefix: &str) -> Vec<String> {
    installed().volume_ids_with_listings(prefix)
}

/// `Inner::get_listing_stats` on the installed engine.
// shim: removed in M4b
pub fn get_listing_stats(
    listing_id: &str,
    include_hidden: bool,
    selected_indices: Option<&[usize]>,
) -> Result<ListingStats, ListingLookupError> {
    installed().get_listing_stats(listing_id, include_hidden, selected_indices)
}

/// `Inner::refresh_listing_index_sizes` on the installed engine.
// shim: removed in M4b
pub fn refresh_listing_index_sizes(listing_id: &str) -> Result<(), String> {
    installed().refresh_listing_index_sizes(listing_id)
}
