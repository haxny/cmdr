//! Quick filter: setting the pattern a pane narrows its rows to while the user
//! types, and carrying its cursor and selection into the new row space. What a
//! pattern matches is `name_filter.rs`'s; this module is the pane setting.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::file_system::listing::engine::Inner;
use crate::file_system::listing::name_filter::NameFilter;
use crate::file_system::listing::operations::ListingLookupError;

// shim: removed in M4b. Callers outside the engine name these here; they forward to the installed engine.
pub use crate::file_system::listing::shims::quick_filter::*;

/// Where the pane's cursor and selection land after a quick-filter change, in
/// the new row space.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NameFilterResult {
    /// Whether the listing took the new pattern. `false` only when the caller
    /// asked to refuse a pattern nothing matches: the listing then keeps its
    /// previous filter, and the rest of this answer describes that one.
    pub accepted: bool,
    /// How many rows the pane shows under the new filter.
    pub total_count: usize,
    /// The row of the file that was under the cursor, or `None` when the new
    /// filter leaves it out (or no file was given).
    pub new_cursor_index: Option<usize>,
    /// The rows of the previously selected files the new filter still shows. A
    /// selected file the filter leaves out drops out of the selection, so no
    /// operation ever acts on a row the user can't see.
    pub new_selected_indices: Vec<usize>,
    /// The diff sequence the new row space starts at, when the filter changed. Every
    /// `directory-diff` numbered up to it describes the old rows: the pane takes it
    /// as its last applied sequence and skips them. `None` when nothing changed.
    pub sequence: Option<u64>,
}

impl Inner {
    /// Sets (or, with an empty or `None` pattern, clears) the pane's quick filter,
    /// and carries the cursor and selection into the new row space, all under ONE
    /// write lock so no diff or read can land between the old row space and the new.
    ///
    /// With `refuse_empty`, a pattern that matches no row is refused and the old
    /// filter stays (`accepted: false`): Total Commander's rule that typing only
    /// ever narrows down to the last match, never past it. Deciding it here, under
    /// the same lock, is what makes the refusal exact; the frontend can't know the
    /// count before asking.
    ///
    /// Superseded pending transitions are dropped under the cache write lock, before
    /// a watcher can publish a transition in the new row space.
    #[cfg(test)]
    pub(crate) fn set_listing_name_filter(
        &self,
        listing_id: &str,
        pattern: Option<&str>,
        include_hidden: bool,
        cursor_filename: Option<&str>,
        selected_indices: &[usize],
        refuse_empty: bool,
    ) -> Result<NameFilterResult, ListingLookupError> {
        self.set_listing_name_filter_inner(
            listing_id,
            pattern,
            include_hidden,
            cursor_filename,
            selected_indices,
            refuse_empty,
            None,
            #[cfg(test)]
            || {},
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Filter reconfiguration carries a guarded selection"
    )]
    pub(crate) fn set_listing_name_filter_guarded(
        &self,
        listing_id: &str,
        pattern: Option<&str>,
        include_hidden: bool,
        cursor_filename: Option<&str>,
        selected_indices: &[usize],
        refuse_empty: bool,
        expected_sequence: Option<u64>,
    ) -> Result<NameFilterResult, ListingLookupError> {
        self.set_listing_name_filter_inner(
            listing_id,
            pattern,
            include_hidden,
            cursor_filename,
            selected_indices,
            refuse_empty,
            expected_sequence,
            #[cfg(test)]
            || {},
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Filter reconfiguration carries a guarded selection and test hook"
    )]
    pub(crate) fn set_listing_name_filter_inner(
        &self,
        listing_id: &str,
        pattern: Option<&str>,
        include_hidden: bool,
        cursor_filename: Option<&str>,
        selected_indices: &[usize],
        refuse_empty: bool,
        expected_sequence: Option<u64>,
        #[cfg(test)] after_unlock: impl FnOnce(),
    ) -> Result<NameFilterResult, ListingLookupError> {
        let visibility = self.scratch_visibility();
        let result = {
            let mut cache = self.write_cache();
            let (listing, publish) = cache
                .get_mut(listing_id)
                .ok_or_else(|| ListingLookupError::gone(listing_id))?;
            listing.reconcile_scratch(listing_id, visibility, &publish);
            super::operations::check_expected(listing_id, listing, expected_sequence)?;
            if expected_sequence.is_some() && listing.include_hidden() != include_hidden {
                return Err(ListingLookupError::changed(listing_id));
            }
            listing.touch();

            let selected_names: Vec<String> = {
                let rows = listing.rows(include_hidden);
                selected_indices
                    .iter()
                    .filter_map(|&row| rows.get(row).map(|entry| entry.name.clone()))
                    .collect()
            };

            let previous = listing.name_filter().cloned();
            let next = pattern.and_then(NameFilter::new);
            // Asked of every entry, not of the current rows: the new pattern needn't
            // narrow the old one, so a row the old filter hides may be its only match.
            let refused = refuse_empty
                && next.is_some()
                && next != previous
                && !listing
                    .entries()
                    .iter()
                    .any(|entry| listing.shows_with_filter(entry, include_hidden, next.as_ref()));
            let changed = !refused && listing.set_name_filter(next);
            // Under the write lock, so no diff can be sequenced between the switch and this.
            let sequence = if changed {
                let sequence = listing.advance_sequence();
                publish.drop_pending(listing_id);
                Some(sequence)
            } else {
                expected_sequence
            };

            let rows = listing.rows(include_hidden);
            let names_to_rows: HashMap<&str, usize> = rows
                .iter()
                .enumerate()
                .map(|(row, entry)| (entry.name.as_str(), row))
                .collect();
            NameFilterResult {
                accepted: !refused,
                total_count: rows.len(),
                new_cursor_index: cursor_filename.and_then(|name| names_to_rows.get(name).copied()),
                new_selected_indices: selected_names
                    .iter()
                    .filter_map(|name| names_to_rows.get(name.as_str()).copied())
                    .collect(),
                sequence,
            }
        };
        #[cfg(test)]
        after_unlock();
        Ok(result)
    }
}
