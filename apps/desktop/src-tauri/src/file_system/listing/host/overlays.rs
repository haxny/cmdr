//! Rows a pane sees that no volume holds.
//!
//! Why this is a listing seam and ❌ never a `Volume` impl: `crate::listing_overlays`.

use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

/// A source of extra rows for one kind of directory.
pub trait ListingOverlay: Send + Sync + 'static {
    /// The contributor's stable name (`"git-portal"`), the key the registry
    /// dedupes on.
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
