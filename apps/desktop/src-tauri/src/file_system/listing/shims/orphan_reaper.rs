//! Shims over the installed engine for `orphan_reaper`'s callers (`shims/mod.rs`).

#[cfg(test)]
use super::installed;

/// `Inner::reap_orphaned_listings_at_for` on the installed engine.
// shim: removed in M4b
#[cfg(test)]
pub(crate) fn reap_orphaned_listings_at_for(now_ms: u64, window_ms: u64, only: &[&str]) -> Vec<String> {
    installed().reap_orphaned_listings_at_for(now_ms, window_ms, only)
}
