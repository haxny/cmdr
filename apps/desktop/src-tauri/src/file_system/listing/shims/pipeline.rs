//! Shims over the installed engine for `pipeline`'s callers (`shims/mod.rs`).

use std::path::Path;

use cmdr_fs::entry::FileEntry;

use super::installed;
use crate::file_system::listing::pipeline::{Prepared, ReadBy};

/// `Inner::prepare` on the installed engine.
// shim: removed in M4b
pub(crate) async fn prepare(read_by: ReadBy<'_>, volume_id: &str, path: &Path, entries: Vec<FileEntry>) -> Prepared {
    installed().prepare(read_by, volume_id, path, entries).await
}
