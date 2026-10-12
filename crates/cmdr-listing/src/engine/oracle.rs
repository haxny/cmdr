//! Lookups: which listings show what, and what a listing holds.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::ListingLookupError;
use crate::ListingSummary;
use crate::listing_id::ListingId;

use super::Listings;

impl Listings {
    /// The entries of a fresh, watched, undecorated listing of `path` on
    /// `volume_id`, or `None` when no listing can vouch for the directory as it
    /// is on disk now. A write operation's scan reads this instead of the disk.
    pub fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.inner.try_get_authoritative_listing(volume_id, path)
    }

    /// The newest pane listing's entries for `path` on `volume_id`, watched or
    /// not. No filesystem call.
    pub fn cached_entries(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.inner.get_cached_listing(volume_id, path)
    }

    /// The listings showing `path` on `volume_id`, matched in that volume's
    /// spelling.
    pub fn listings_for_path_on_volume(&self, volume_id: &str, path: &Path) -> Vec<ListingId> {
        ids(self.inner.find_listings_for_path_on_volume(Some(volume_id), path))
    }

    /// The listings showing `path`, compared verbatim across EVERY volume, so a
    /// remote pane showing the same path string matches too. Only for a local
    /// caller that doesn't know its own volume id.
    pub fn listings_for_local_path(&self, path: &Path) -> Vec<ListingId> {
        ids(self.inner.find_listings_for_path_on_volume(None, path))
    }

    /// The volume id and directory `id` lists.
    pub fn location(&self, id: &ListingId) -> Option<(String, PathBuf)> {
        self.inner.get_listing_volume_id_and_path(id.as_str())
    }

    /// Whether the engine holds `id`.
    pub fn contains(&self, id: &ListingId) -> bool {
        self.inner.contains(id.as_str())
    }

    /// The distinct volume ids under `prefix` that some listing shows.
    pub fn volume_ids_with_listings(&self, prefix: &str) -> Vec<String> {
        self.inner.volume_ids_with_listings(prefix)
    }

    /// Owned copies of `id`'s folder rows (non-symlink directories) named in
    /// `names`, in listing order: the rows the index holds sizes for. Doesn't
    /// count as the pane using the listing.
    pub fn folder_entries_named(
        &self,
        id: &ListingId,
        names: &BTreeSet<String>,
    ) -> Result<Vec<FileEntry>, ListingLookupError> {
        let cache = self.inner.cache.read_ignore_poison();
        let listing = cache
            .get(id.as_str())
            .ok_or_else(|| ListingLookupError::gone(id.as_str()))?;
        Ok(listing
            .entries()
            .iter()
            .filter(|entry| entry.is_directory && !entry.is_symlink && names.contains(&entry.name))
            .cloned()
            .collect())
    }

    /// One line per open listing, youngest first.
    pub fn summaries(&self) -> Vec<ListingSummary> {
        self.inner.snapshot_listings()
    }
}

/// The ids of a lookup's matches; their sort settings stay inside.
fn ids<A, B, C>(found: Vec<(String, A, B, C)>) -> Vec<ListingId> {
    found.into_iter().map(|(id, ..)| ListingId::new(id)).collect()
}
