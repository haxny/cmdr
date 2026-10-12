//! Row-space reads: every index here is a row of the pane asking, never a
//! position in the listing's entries, which no caller sees.

use std::collections::HashMap;
use std::path::PathBuf;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::compare::{CompareDirectoriesMode, CompareDirectoriesResult};
use crate::fuzzy_jump::FuzzyJumpError;
use crate::listing_id::ListingId;
use crate::stats::ListingStats;
use crate::visible_rows::VisibleRows;
use crate::{ListingLookupError, RowBeside, SelectionSnapshot};

use super::Listings;

impl Listings {
    /// Up to `count` rows from row `start`.
    pub fn file_range(
        &self,
        id: &ListingId,
        start: usize,
        count: usize,
        include_hidden: bool,
    ) -> Result<Vec<FileEntry>, ListingLookupError> {
        self.inner.get_file_range(id.as_str(), start, count, include_hidden)
    }

    /// The row named `name`, or a unique look-alike spelling of it: where the
    /// cursor lands.
    pub fn find_file_index(
        &self,
        id: &ListingId,
        name: &str,
        include_hidden: bool,
    ) -> Result<Option<usize>, ListingLookupError> {
        self.inner.find_file_index(id.as_str(), name, include_hidden)
    }

    /// The rows of the names found, by name.
    pub fn find_file_indices(
        &self,
        id: &ListingId,
        names: &[String],
        include_hidden: bool,
    ) -> Result<HashMap<String, usize>, ListingLookupError> {
        self.inner.find_file_indices(id.as_str(), names, include_hidden)
    }

    /// The row before or after the one named `name`, read against one snapshot.
    pub fn file_beside(
        &self,
        id: &ListingId,
        name: &str,
        side: RowBeside,
        include_hidden: bool,
    ) -> Result<Option<FileEntry>, ListingLookupError> {
        self.inner.get_file_beside(id.as_str(), name, side, include_hidden)
    }

    /// The entry on row `row`.
    pub fn file_at(
        &self,
        id: &ListingId,
        row: usize,
        include_hidden: bool,
    ) -> Result<Option<FileEntry>, ListingLookupError> {
        self.inner.get_file_at(id.as_str(), row, include_hidden)
    }

    /// The paths on the pane's rows `rows`. With `has_parent`, the pane's row 0
    /// is its `..` row: skipped, and every other row shifts down by one.
    pub fn paths_at_indices(
        &self,
        id: &ListingId,
        rows: &[usize],
        include_hidden: bool,
        has_parent: bool,
    ) -> Result<Vec<PathBuf>, ListingLookupError> {
        self.inner
            .get_paths_at_indices(id.as_str(), rows, include_hidden, has_parent)
    }

    /// The entries on rows `rows` (no `..` offset: the caller adjusts).
    pub fn files_at_indices(
        &self,
        id: &ListingId,
        rows: &[usize],
        include_hidden: bool,
    ) -> Result<Vec<FileEntry>, ListingLookupError> {
        self.inner.get_files_at_indices(id.as_str(), rows, include_hidden)
    }

    /// The pane's totals, and the selection's when `selected` names rows.
    pub fn stats(
        &self,
        id: &ListingId,
        include_hidden: bool,
        selected: Option<&[usize]>,
    ) -> Result<ListingStats, ListingLookupError> {
        self.inner.get_listing_stats(id.as_str(), include_hidden, selected)
    }

    /// The paths and counts of the selected rows, only while the pane's rows
    /// are still the committed ones (`expected_sequence`, and the pane's own
    /// hidden-files setting).
    pub fn selection_snapshot(
        &self,
        id: &ListingId,
        include_hidden: bool,
        selected: &[usize],
        expected_sequence: u64,
    ) -> Result<SelectionSnapshot, ListingLookupError> {
        self.inner
            .get_selection_snapshot(id.as_str(), include_hidden, selected, expected_sequence)
    }

    /// The names on the selected rows (every row with `None`), under the same
    /// guard as [`selection_snapshot`](Self::selection_snapshot).
    pub fn selection_names(
        &self,
        id: &ListingId,
        include_hidden: bool,
        selected: Option<&[usize]>,
        expected_sequence: u64,
    ) -> Result<Vec<String>, ListingLookupError> {
        self.inner
            .get_selection_names(id.as_str(), include_hidden, selected, expected_sequence)
    }

    /// Every entry the listing holds, hidden and scratch rows included, as an
    /// owned copy.
    pub fn entries(&self, id: &ListingId) -> Result<Vec<FileEntry>, ListingLookupError> {
        // Every entry, so no scratch reconcile: that only decides which are rows.
        let cache = self.inner.cache.read_ignore_poison();
        let listing = cache
            .get(id.as_str())
            .ok_or_else(|| ListingLookupError::gone(id.as_str()))?;
        listing.touch();
        Ok(listing.entries().to_vec())
    }

    /// Runs `f` over the rows the pane shows, for a host feature that lays them
    /// out (Brief columns). It runs under the cache read lock, so it must be
    /// quick and ❌ must never call back into the engine.
    pub fn with_rows<R>(
        &self,
        id: &ListingId,
        include_hidden: bool,
        f: impl FnOnce(&VisibleRows<'_>) -> R,
    ) -> Result<R, ListingLookupError> {
        let cache = self.inner.reconciled_cache(&[id.as_str()]);
        let listing = cache
            .get(id.as_str())
            .ok_or_else(|| ListingLookupError::gone(id.as_str()))?;
        Ok(f(&listing.rows(include_hidden)))
    }

    /// Which rows each pane marks against the other (⇧F2).
    pub fn compare_directories(
        &self,
        left: (&ListingId, bool),
        right: (&ListingId, bool),
        mode: CompareDirectoriesMode,
    ) -> Result<CompareDirectoriesResult, ListingLookupError> {
        self.inner
            .compare_directories(left.0.as_str(), left.1, right.0.as_str(), right.1, mode)
    }

    /// The row of the best fuzzy match for `query` (type-to-jump).
    pub fn fuzzy_find_first_match(
        &self,
        id: &ListingId,
        query: &str,
        include_hidden: bool,
    ) -> Result<Option<usize>, FuzzyJumpError> {
        self.inner
            .fuzzy_find_first_match_in_listing(id.as_str(), query, include_hidden)
    }
}
