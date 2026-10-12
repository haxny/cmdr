//! The free functions callers outside the engine (and the tests) still name,
//! forwarding to the app's installed engine. One file per engine module;
//! each engine module re-exports its own, so every caller's path is unchanged.
//!
//! Temporary by design: M4a moves the callers onto `Listings`' public API, M4b
//! gives every test its own engine, and this file goes. ❌ Engine code never
//! calls these: it reaches its state through `self`.

use std::sync::Arc;

use super::engine::Inner;

/// The app's installed engine.
pub(super) fn installed() -> &'static Arc<Inner> {
    super::installed()
}

pub(crate) mod cached_listing;
pub(crate) mod caching;
pub(crate) mod compare;
pub(crate) mod diff_emitter;
pub(crate) mod foreign_path;
pub(crate) mod fuzzy_jump;
pub(crate) mod operations;
pub(crate) mod orphan_reaper;
pub(crate) mod overlays;
pub(crate) mod quick_filter;
pub(crate) mod sorting;
pub(crate) mod streaming;
