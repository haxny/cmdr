//! The name of one open listing, as a type of its own.
//!
//! A listing id and a volume id are both strings, and they sit side by side in
//! the engine's signatures. As two `&str`s they're interchangeable to the
//! compiler; as a [`ListingId`] and a `&str` they aren't. Only the IPC boundary
//! sees a bare `String`, which it wraps on the way in, so the generated bindings
//! never name this type.

use std::fmt;

/// One open listing's id: the frontend mints it per navigation, and the engine
/// keys the listing's cache entry, its diffs, and its events on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ListingId(String);

impl ListingId {
    /// Wraps an id the frontend (or a test) minted.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id as the frontend spells it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ListingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
