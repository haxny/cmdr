//! Rows a PANE shows that no volume holds.
//!
//! A [`ListingOverlay`] contributes extra [`FileEntry`]s to one directory's
//! listing, folded in by the listing pipeline after the volume's own entries
//! arrive. Cmdr's one contributor is the virtual `.git` portal, which puts the
//! six category rows (`branches/`, `tags/`, …) into a repo's `.git/` listing
//! while every real entry beside them stays the local volume's, editable and
//! deletable as always.
//!
//! ❌ **Never move this into a `Volume` impl**: a contributed row must reach a
//! pane and nothing else, which is also why a decorated listing never answers
//! the fresh-listing oracle. Why: `host/DETAILS.md` § "Overlays".
//!
//! The host hands its contributors to the engine's builder, in order
//! (`ListingsBuilder::overlay`), and the read pipeline (`pipeline.rs`) folds
//! over whatever the engine holds, between the enrich and the sort, on every
//! read that builds or replaces a listing.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

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
