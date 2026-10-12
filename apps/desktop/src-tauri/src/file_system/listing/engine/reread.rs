//! Re-reads: the watcher's and ⌘R's full re-read, the host's own read when
//! nothing serves a listing's volume, and the watcher's per-child changes. The
//! engine half is `reread.rs`.

use std::path::PathBuf;

use cmdr_fs::entry::FileEntry;

use crate::file_system::listing::listing_id::ListingId;
use crate::file_system::listing::reread::{ReadTicket, RefreshOutcome};

use super::Listings;

impl Listings {
    /// Re-reads `id`'s directory through its volume and replaces the listing's
    /// entries, publishing what changed. Returns once the re-read landed or
    /// didn't, never earlier: E2E's watcher flush and ⌘R's bounded wait both
    /// rely on that.
    pub async fn refresh(&self, id: &ListingId) -> RefreshOutcome {
        self.inner.refresh(id).await
    }

    /// Starts a re-read the host does itself (nothing serves the listing's
    /// volume id), or `None` when the listing is gone and there's nothing to read for.
    pub fn begin_read(&self, id: &ListingId) -> Option<ReadTicket> {
        self.inner.begin_read(id)
    }

    /// Hands over what the host read for `ticket`, replacing the listing's
    /// entries. The overlays don't run (nothing serves the volume to decorate
    /// through), so the listing keeps its overlay count.
    pub async fn refresh_from_entries(&self, ticket: ReadTicket, entries: Vec<FileEntry>) -> RefreshOutcome {
        self.inner.refresh_from_entries(ticket, entries).await
    }

    /// Patches `id` with the host's fresh stat of some of its children: `None`
    /// for a child that's gone. The engine sorts them into removes, adds, and
    /// modifies against what the listing holds, enriches the adds and modifies
    /// in one call, carries Finder tags forward on a modify, and publishes in
    /// that order: removes first, since their rows are the old pane's.
    pub fn observe_children(&self, id: &ListingId, children: Vec<(PathBuf, Option<FileEntry>)>) {
        self.inner.observe_children(id, children);
    }
}
