//! What the host starts once and asks the engine for its own needs.

use std::sync::Arc;

use cmdr_collation::NameCollator;

use super::Listings;

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

    /// Sends every queued diff batch now, for an end-to-end test that waits on
    /// the pane.
    #[cfg(feature = "playwright-e2e")]
    pub fn flush_pending_diffs(&self) {
        self.inner.diffs.flush_all_pending();
    }
}
