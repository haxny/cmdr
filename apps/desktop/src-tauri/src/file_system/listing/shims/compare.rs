//! Shims over the installed engine for `compare`'s callers (`shims/mod.rs`).

use super::installed;
use crate::file_system::listing::ListingLookupError;
use crate::file_system::listing::compare::{CompareDirectoriesMode, CompareDirectoriesResult};

/// `Inner::compare_directories` on the installed engine.
// shim: removed in M4b
pub fn compare_directories(
    left_listing_id: &str,
    left_include_hidden: bool,
    right_listing_id: &str,
    right_include_hidden: bool,
    mode: CompareDirectoriesMode,
) -> Result<CompareDirectoriesResult, ListingLookupError> {
    installed().compare_directories(
        left_listing_id,
        left_include_hidden,
        right_listing_id,
        right_include_hidden,
        mode,
    )
}
