//! The directory listing engine: [`Listings`] and everything a host programs
//! against. The re-exports below ARE the API; every submodule but `host` is
//! private, so nothing outside can reach past them.

mod cached_listing;
mod caching;
mod collation;
mod compare;
mod diff;
mod diff_emitter;
mod engine;
mod foreign_path;
mod fuzzy_jump;
pub(crate) mod host;
mod index_sizes;
mod listing_id;
mod name_filter;
mod operations;
mod orphan_reaper;
mod path_index;
mod pipeline;
mod quick_filter;
mod reread;
#[cfg(test)]
mod shims;
mod sorting;
mod stall;
mod stalled_on;
mod stats;
mod streaming;
mod visible_rows;

// The public API names a few types the app only meets as values (returned, or
// inside another type); M5 makes this the crate root, where none is unused.
#[allow(unused_imports, reason = "public API the app meets only as values")]
pub use caching::ListingSummary;
#[allow(unused_imports, reason = "public API the app meets only as values")]
pub use collation::NameCollator;
pub use compare::{CompareDirectoriesError, CompareDirectoriesMode, CompareDirectoriesResult};
#[allow(unused_imports, reason = "public API the app meets only as values")]
pub use diff::{DiffChange, DiffChangeType, DirectoryDiffBatch};
pub use engine::{Listings, ListingsBuilder, PaneMarks};
pub use foreign_path::stored_spellings;
pub use fuzzy_jump::FuzzyJumpError;
pub use index_sizes::IndexSizes;
pub use listing_id::ListingId;
pub use operations::{ListingLookupError, ResortResult, RowBeside, SelectionSnapshot};
pub use quick_filter::NameFilterResult;
#[allow(unused_imports, reason = "public API the app meets only as values")]
pub use reread::{ReadTicket, RefreshOutcome};
pub use sorting::{
    DirectorySortMode, SortColumn, SortOrder, SortableEntry, entry_comparator_with as entry_comparator,
    sort_entries_with as sort_entries,
};
pub use stalled_on::StalledOn;
pub use stats::ListingStats;
pub use streaming::ListingRequest;
pub use visible_rows::VisibleRows;

// Test shims and test-only names at the root, for tests (inside and outside the
// engine) that still drive the one shared engine. M4b deletes them with the
// shims, once every test builds its own engine.
#[cfg(test)]
pub(crate) use {
    cached_listing::listing_cache,
    caching::try_get_authoritative_listing,
    diff_emitter::{hold_for_test, pending_changes_for_test},
    fuzzy_jump::fuzzy_find_first_match_in_listing,
    operations::{list_directory_end, list_directory_start_with_volume, update_listing_entries},
    pipeline::OverlayRows,
    streaming::{StreamingListingState, cancel_listing, read_directory_with_progress, streaming_state},
};

/// The app's installed engine, for the test shims that still forward to it.
/// ❌ Never call it from engine code, which reaches its own state through `self`.
// shim: removed in M4b
#[cfg(test)]
pub(crate) fn installed() -> &'static std::sync::Arc<engine::Inner> {
    crate::listing_host::listings().inner()
}

// The app's half of `cmdr-archive`'s live-content watch: what a refresh does to
// the listing cache, which is this module's side of the seam.
#[cfg(test)]
mod archive_watch_integration_test;
#[cfg(test)]
mod caching_oracle_test;
#[cfg(test)]
mod caching_reaper_test;
#[cfg(test)]
mod caching_replacement_test;
#[cfg(test)]
mod caching_test;
#[cfg(test)]
pub(crate) mod caching_test_support;
// Name order itself, split from `sorting_test` because it pins the process-wide
// reading language and shares that lock with `intl::native_strings`.
#[cfg(test)]
mod collation_test;
#[cfg(test)]
mod compare_test;
#[cfg(test)]
mod diff_emitter_test;
#[cfg(test)]
mod diff_test;
#[cfg(test)]
mod engine_test;
#[cfg(test)]
mod find_file_index_test;
#[cfg(test)]
mod fuzzy_jump_test;
#[cfg(test)]
mod hidden_files_test;
#[cfg(test)]
mod index_sizes_test;
#[cfg(test)]
mod lifecycle_order_test;
#[cfg(all(test, feature = "virtual-mtp"))]
mod mtp_listing_path_test;
#[cfg(test)]
mod name_filter_test;
#[cfg(test)]
mod operations_test;
#[cfg(test)]
mod overlay_read_paths_test;
#[cfg(test)]
mod overlay_refresh_test;
#[cfg(test)]
mod pane_diff_test;
#[cfg(test)]
mod path_index_test;
#[cfg(test)]
mod probe_test_support;
#[cfg(test)]
mod read_generation_test;
#[cfg(test)]
mod row_beside_test;
#[cfg(test)]
pub(crate) mod seam_test_support;
// A pane close drops a listing, and must not reach the volume's own watcher.
// Docker-gated, over a real `cmdr-smb` session, because the SMB watcher's
// lifetime is what the regression was about.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod smb_pane_close_watch_integration_test;
#[cfg(test)]
mod sorting_dir_mode_test;
#[cfg(test)]
mod sorting_test;
#[cfg(test)]
mod sorting_test_support;
#[cfg(test)]
mod staging_temps_test;
#[cfg(test)]
mod stall_test;
#[cfg(test)]
mod stats_test;
#[cfg(test)]
mod streaming_events_test;
#[cfg(test)]
mod streaming_reaction_test;
#[cfg(test)]
mod streaming_test;
#[cfg(test)]
mod visible_rows_test;
