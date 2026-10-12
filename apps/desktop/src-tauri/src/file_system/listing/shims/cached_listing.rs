//! Shims over the installed engine for `cached_listing`'s callers (`shims/mod.rs`).

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard};

use super::installed;
use crate::file_system::listing::cached_listing::CachedListing;

/// The installed engine's cache, for callers outside the engine that still
/// reach into it.
// shim: removed in M4b
pub(crate) fn listing_cache() -> &'static RwLock<HashMap<String, CachedListing>> {
    &installed().cache
}

/// `Inner::reconciled_cache` on the installed engine.
// shim: removed in M4b
pub(crate) fn reconciled_cache(ids: &[&str]) -> RwLockReadGuard<'static, HashMap<String, CachedListing>> {
    installed().reconciled_cache(ids)
}

/// `Inner::epoch_millis_now` on the installed engine.
// shim: removed in M4b
#[cfg(test)]
pub(crate) fn epoch_millis_now() -> u64 {
    installed().epoch_millis_now()
}
