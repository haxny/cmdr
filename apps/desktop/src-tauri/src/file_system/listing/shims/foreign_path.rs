//! Shims over the installed engine for `foreign_path`'s callers (`shims/mod.rs`).

use std::path::PathBuf;

use super::installed;

/// `Inner::respell_listing` on the installed engine.
// shim: removed in M4b
pub(crate) async fn respell_listing(volume_id: String, listing_id: String, path: PathBuf) {
    installed().respell_listing(volume_id, listing_id, path).await;
}
