//! Polling for changes: the one place this backend breaks "subscribe, don't
//! poll", because NFSv3 has nothing to subscribe to.
//!
//! One `GETATTR` per directory a pane shows on the volume, every 3 s,
//! re-listing only when the directory's `mtime` or `ctime` moved; nothing at
//! all with no pane open on the volume, and nothing while it's disconnected.
//! The plan's § "Product decisions" and its M7 have the rest.

use std::path::Path;

use super::NfsVolume;

impl NfsVolume {
    /// A listing of `path` just succeeded, so a pane may be showing it: the
    /// poller's cue to start, if it isn't running.
    pub(super) fn listing_opened(&self, path: &Path) {
        let _ = path;
    }
}
