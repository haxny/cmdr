//! Shims over the installed engine for `foreign_path`'s callers (`shims/mod.rs`).

#[cfg(test)]
use std::path::PathBuf;

use super::installed;

/// `Inner::respell_listings_on_volume` on the installed engine.
// shim: removed in M4b
pub(crate) fn respell_listings_on_volume(volume_id: &str) {
    installed().respell_listings_on_volume(volume_id);
}

/// `Inner::respell_listing` on the installed engine.
// shim: removed in M4b
#[cfg(test)]
pub(crate) async fn respell_listing(volume_id: String, listing_id: String, path: PathBuf) {
    installed().respell_listing(volume_id, listing_id, path).await;
}
