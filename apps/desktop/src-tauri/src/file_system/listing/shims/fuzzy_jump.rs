//! Shims over the installed engine for `fuzzy_jump`'s callers (`shims/mod.rs`).

use super::installed;
use crate::file_system::listing::fuzzy_jump::FuzzyJumpError;

/// `Inner::fuzzy_find_first_match_in_listing` on the installed engine.
// shim: removed in M4b
pub fn fuzzy_find_first_match_in_listing(
    listing_id: &str,
    query: &str,
    include_hidden: bool,
) -> Result<Option<usize>, FuzzyJumpError> {
    installed().fuzzy_find_first_match_in_listing(listing_id, query, include_hidden)
}
