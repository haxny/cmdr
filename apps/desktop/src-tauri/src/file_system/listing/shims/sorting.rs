//! Shims over the installed engine for `sorting`'s callers (`shims/mod.rs`).

use cmdr_fs::entry::FileEntry;

use super::installed;
use crate::file_system::listing::sorting::{
    DirectorySortMode, SortColumn, SortOrder, SortableEntry, entry_comparator_with, sort_entries_with,
};

/// `entry_comparator_with` in the installed engine's reading language.
// shim: removed in M4b
pub fn entry_comparator<E: SortableEntry + ?Sized>(
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> impl Fn(&E, &E) -> std::cmp::Ordering {
    entry_comparator_with(installed().collator(), sort_by, sort_order, dir_sort_mode)
}

/// `sort_entries_with` in the installed engine's reading language.
// shim: removed in M4b
pub fn sort_entries(
    entries: &mut [FileEntry],
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) {
    sort_entries_with(&installed().collator(), entries, sort_by, sort_order, dir_sort_mode);
}
