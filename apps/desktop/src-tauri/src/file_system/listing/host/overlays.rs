//! Rows a PANE shows that no volume holds.
//!
//! A [`ListingOverlay`] contributes extra [`FileEntry`]s to one directory's
//! listing, folded in by the listing pipeline after the volume's own entries
//! arrive. Cmdr's one contributor is the virtual `.git` portal, which puts the
//! six category rows (`branches/`, `tags/`, …) into a repo's `.git/` listing
//! while every real entry beside them stays the local volume's, editable and
//! deletable as always.
//!
//! ❌ **Never move this into a `Volume` impl or into the volume manager.** The
//! whole point is that a contributed row reaches a pane and NOTHING else: a copy
//! scan, a delete walker, and the drive indexer all list through `Volume`, and
//! the moment one of them can see a row with no inode behind it, a repo delete
//! stops half-way through with `.git/` still on disk. That was a real bug, and
//! this seam is the shape that makes it unrepresentable.
//!
//! The same rule is why a listing that carries contributed rows is never
//! authoritative for a walker: `CachedListing::has_overlay_rows` says so and the
//! fresh-listing oracle declines it (`listing/caching.rs`).
//!
//! The host hands its contributors to the engine's builder, in order
//! (`ListingsBuilder::overlay`), and the read pipeline (`pipeline.rs`) folds
//! over whatever the engine holds, between the enrich and the sort, on every
//! read that builds or replaces a listing.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

// shim: removed in M4b. The tests that pin the overlay step alone name it here.
#[cfg(test)]
pub(crate) use crate::file_system::listing::pipeline::test_support::decorate;

/// A source of extra rows for one kind of directory.
pub trait ListingOverlay: Send + Sync + 'static {
    /// The contributor's stable name (`"git-portal"`), the key the slot dedupes
    /// on.
    fn id(&self) -> &'static str;

    /// Whether this overlay has anything to say about `path` on `volume`.
    ///
    /// ❗ Cheap and zero-I/O: this runs on EVERY directory listing in the app,
    /// so it has to be a look at the path and the volume's capability flags.
    /// The expensive half is [`extra_entries`](Self::extra_entries), which only
    /// runs when this answered `true`.
    fn applies_to(&self, volume: &dyn Volume, path: &Path) -> bool;

    /// The rows to fold in. Runs on the blocking pool, so it may open a
    /// repository or hit the disk.
    fn extra_entries(&self, volume: &dyn Volume, path: &Path) -> Vec<FileEntry>;
}
