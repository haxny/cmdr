//! Shims over the installed engine for `overlays`'s callers (`shims/mod.rs`).

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

use super::installed;

/// `Inner::decorate` on the app's installed engine.
// shim: removed in M4b
pub(crate) async fn decorate(volume: &Arc<dyn Volume>, path: &Path, entries: &mut Vec<FileEntry>) -> usize {
    installed().decorate(volume, path, entries).await
}
