//! Shims over the installed engine for `quick_filter`'s callers (`shims/mod.rs`).

use super::installed;
use crate::file_system::listing::ListingLookupError;
use crate::file_system::listing::quick_filter::NameFilterResult;

/// `Inner::set_listing_name_filter` on the installed engine.
// shim: removed in M4b
pub fn set_listing_name_filter(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
) -> Result<NameFilterResult, ListingLookupError> {
    installed().set_listing_name_filter(
        listing_id,
        pattern,
        include_hidden,
        cursor_filename,
        selected_indices,
        refuse_empty,
    )
}

/// `Inner::set_listing_name_filter_guarded` on the installed engine.
// shim: removed in M4b
#[allow(
    clippy::too_many_arguments,
    reason = "Filter reconfiguration carries a guarded selection"
)]
pub fn set_listing_name_filter_guarded(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
    expected_sequence: Option<u64>,
) -> Result<NameFilterResult, ListingLookupError> {
    installed().set_listing_name_filter_guarded(
        listing_id,
        pattern,
        include_hidden,
        cursor_filename,
        selected_indices,
        refuse_empty,
        expected_sequence,
    )
}

/// `Inner::set_listing_name_filter_inner` on the installed engine.
// shim: removed in M4b
#[allow(
    clippy::too_many_arguments,
    reason = "Filter reconfiguration carries a guarded selection and test hook"
)]
pub(crate) fn set_listing_name_filter_inner(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
    expected_sequence: Option<u64>,
    after_unlock: impl FnOnce(),
) -> Result<NameFilterResult, ListingLookupError> {
    installed().set_listing_name_filter_inner(
        listing_id,
        pattern,
        include_hidden,
        cursor_filename,
        selected_indices,
        refuse_empty,
        expected_sequence,
        after_unlock,
    )
}
