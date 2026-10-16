//! The write engine's own-write announces, answered by the downloads watcher's
//! ignore set, so a file Cmdr lands in `~/Downloads` never reads as a new
//! download.

use std::path::Path;

use crate::file_system::write_operations::host::OwnWrites;

/// `downloads::note_pending_write_for_cmdr`, which no-ops while the watcher
/// sleeps and for a path outside the Downloads root.
pub(super) struct AppOwnWrites;

impl OwnWrites for AppOwnWrites {
    fn announce(&self, path: &Path) {
        crate::downloads::note_pending_write_for_cmdr(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloads::own_write_recorder::{Announced, record_under};
    use crate::test_support::TestDir;

    /// An announce reaches the downloads hook, path for path, while nothing
    /// exists there yet.
    #[test]
    fn an_announce_reaches_the_downloads_hook() {
        let dir = TestDir::new("operations_host_own_writes");
        let recording = record_under(&dir);
        let path = dir.join("incoming.bin");

        AppOwnWrites.announce(&path);

        assert_eq!(recording.announced(), vec![Announced { path, existed: false }]);
    }
}
