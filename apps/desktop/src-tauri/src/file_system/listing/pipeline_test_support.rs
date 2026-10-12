//! The overlay step alone, for the tests that pin what a contributor folds in
//! (the shadowing rule, the git portal's six rows). Production reaches it only
//! through `Inner::prepare`.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

/// The installed engine's overlay step on `entries`; reports how many rows it
/// added.
// shim: removed in M4b
pub(crate) async fn decorate(volume: &Arc<dyn Volume>, path: &Path, entries: &mut Vec<FileEntry>) -> usize {
    crate::file_system::listing::installed()
        .decorate(volume, path, entries)
        .await
}
