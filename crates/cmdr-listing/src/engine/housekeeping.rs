//! What the host starts once and asks the engine for its own needs.

use std::sync::Arc;

use cmdr_fs::volume::host::listings::ListingHost;

use crate::collation::NameCollator;

use super::Listings;
use super::backends::EngineListingHost;

impl Listings {
    /// Starts the backstop that reclaims listings whose close never arrived
    /// (`orphan_reaper.rs`). Call once. The loop holds this engine weakly, so it
    /// ends once the last handle is dropped.
    pub fn start_orphan_reaper(&self) {
        super::Inner::start_orphan_reaper(&self.inner);
    }

    /// The collator names sort by in the host's reading language now, for a
    /// host feature that sorts rows the way a listing does (search results).
    pub fn collator(&self) -> Arc<NameCollator> {
        self.inner.collator()
    }

    /// This engine as a storage backend's `ListingHost`, panes only: a backend
    /// built with it patches, asks, and refreshes this engine's listings. It
    /// holds the engine weakly, so a volume keeping it doesn't keep the engine
    /// alive, and answers nothing once the engine is gone.
    pub fn listing_host(&self) -> Arc<dyn ListingHost> {
        Arc::new(EngineListingHost::new(&self.inner))
    }

    /// Sends every queued diff batch now, for an end-to-end test that waits on
    /// the pane.
    #[cfg(feature = "playwright-e2e")]
    pub fn flush_pending_diffs(&self) {
        self.inner.diffs.flush_all_pending();
    }
}
