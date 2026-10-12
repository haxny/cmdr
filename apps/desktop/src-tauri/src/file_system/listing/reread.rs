//! What a host's change watch asks the engine once it heard something: a full
//! re-read of one listing ([`Listings::refresh`], or [`Listings::begin_read`] +
//! [`Listings::refresh_from_entries`] when the host reads the directory itself),
//! or an incremental patch of the children it re-stat'd
//! ([`Listings::observe_children`]).
//!
//! The OS half (debouncing, rebasing event paths, the stat, when to fall back to
//! a full re-read, telling the pane its directory is gone) stays with the host.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::VolumeError;

use super::engine::{Inner, Listings};
use super::listing_id::ListingId;
use super::pipeline::ReadBy;

/// How a full re-read of one listing ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// The re-read replaced the listing's entries.
    Landed,
    /// The directory itself is gone (`NotFound`). The host tells the pane and
    /// stops whatever watches it. Carries the listing's path.
    DirectoryGone(PathBuf),
    /// Nothing serves the listing's volume id any more. The host may read the
    /// directory itself: [`Listings::begin_read`], then
    /// [`Listings::refresh_from_entries`].
    NoVolume,
    /// The listing isn't open (any more).
    ListingGone,
    /// The re-read failed and the listing keeps what it had: silently on
    /// `PermissionDenied`, with a warning for anything else.
    Skipped,
}

/// A re-read the host does itself, begun before it reads: what to read, and
/// which listing the entries are for. Not `Clone`: one read, one hand-over.
#[derive(Debug)]
pub struct ReadTicket {
    id: ListingId,
    volume_id: String,
    path: PathBuf,
}

impl ReadTicket {
    /// The directory to read, in the listing's spelling.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Listings {
    /// Re-reads `id`'s directory through its volume and replaces the listing's
    /// entries, publishing what changed. Returns once the re-read landed or
    /// didn't, never earlier: E2E's watcher flush and ⌘R's bounded wait both
    /// rely on that.
    pub async fn refresh(&self, id: &ListingId) -> RefreshOutcome {
        self.inner().refresh(id).await
    }

    /// Starts a re-read the host does itself (nothing serves the listing's
    /// volume id), or `None` when the listing is gone and there's nothing to read for.
    pub fn begin_read(&self, id: &ListingId) -> Option<ReadTicket> {
        self.inner().begin_read(id)
    }

    /// Hands over what the host read for `ticket`, replacing the listing's
    /// entries. The overlays don't run (nothing serves the volume to decorate
    /// through), so the listing keeps its overlay count.
    pub async fn refresh_from_entries(&self, ticket: ReadTicket, entries: Vec<FileEntry>) -> RefreshOutcome {
        self.inner().refresh_from_entries(ticket, entries).await
    }

    /// Patches `id` with the host's fresh stat of some of its children: `None`
    /// for a child that's gone. The engine sorts them into removes, adds, and
    /// modifies against what the listing holds, enriches the adds and modifies
    /// in one call, carries Finder tags forward on a modify, and publishes in
    /// that order: removes first, since their rows are the old pane's.
    pub fn observe_children(&self, id: &ListingId, children: Vec<(PathBuf, Option<FileEntry>)>) {
        self.inner().observe_children(id, children);
    }
}

impl Inner {
    pub(crate) async fn refresh(&self, id: &ListingId) -> RefreshOutcome {
        let Some((volume_id, path)) = self.get_listing_volume_id_and_path(id.as_str()) else {
            return RefreshOutcome::ListingGone;
        };

        // Resolve (not plain `get`) so a `.zip`-crossing listing re-reads through the
        // same ArchiveVolume the listing used, re-registering it if the LRU evicted it.
        let resolved = self.volumes.resolve(&volume_id, &path).await;
        let is_routed = resolved.is_routed();
        let Some(volume) = resolved.volume else {
            return RefreshOutcome::NoVolume;
        };

        let entries = match volume.list_directory(&path, None).await {
            Ok(entries) => entries,
            Err(VolumeError::NotFound(_)) => return RefreshOutcome::DirectoryGone(path),
            Err(VolumeError::PermissionDenied { .. }) => return RefreshOutcome::Skipped,
            Err(e) => {
                log::warn!("Watcher: Failed to re-read directory: {}", e);
                return RefreshOutcome::Skipped;
            }
        };

        // `list_directory` answers with the real directory alone, so without the
        // overlays a ⌘R (or the top-up every copy, move, and delete fires when it
        // settles) would empty a repo's six portal rows out of an open `.git/` pane.
        let read_by = ReadBy::Volume {
            volume: &volume,
            routed: is_routed,
        };
        let prepared = self.prepare(read_by, &volume_id, &path, entries).await;
        self.update_listing_entries(id.as_str(), prepared);
        RefreshOutcome::Landed
    }

    pub(crate) fn begin_read(&self, id: &ListingId) -> Option<ReadTicket> {
        let (volume_id, path) = self.get_listing_volume_id_and_path(id.as_str())?;
        Some(ReadTicket {
            id: id.clone(),
            volume_id,
            path,
        })
    }

    pub(crate) async fn refresh_from_entries(&self, ticket: ReadTicket, entries: Vec<FileEntry>) -> RefreshOutcome {
        let ReadTicket { id, volume_id, path } = ticket;
        let prepared = self.prepare(ReadBy::StdFs, &volume_id, &path, entries).await;
        self.update_listing_entries(id.as_str(), prepared);
        RefreshOutcome::Landed
    }

    pub(crate) fn observe_children(&self, id: &ListingId, children: Vec<(PathBuf, Option<FileEntry>)>) {
        let listing_id = id.as_str();
        let Some((volume_id, _)) = self.get_listing_volume_id_and_path(listing_id) else {
            return;
        };

        let mut adds: Vec<FileEntry> = Vec::new();
        let mut removes: Vec<PathBuf> = Vec::new();
        let mut modifies: Vec<FileEntry> = Vec::new();
        for (path, stat) in children {
            let in_cache = self.has_entry(listing_id, &path.to_string_lossy());
            match (in_cache, stat) {
                (true, Some(entry)) => modifies.push(entry),
                (true, None) => removes.push(path),
                (false, Some(entry)) => adds.push(entry),
                // Not in the listing and gone from disk: nothing to do.
                (false, None) => {}
            }
        }
        if adds.is_empty() && removes.is_empty() && modifies.is_empty() {
            return;
        }

        // ONE enrich for the batch, since a seam is called per mutation, never per
        // entry. Every entry is a direct child of the listing's directory, so the
        // batch resolves one parent.
        let added = adds.len();
        let mut batch: Vec<FileEntry> = adds.into_iter().chain(modifies).collect();
        self.index.enrich(&volume_id, &mut batch);
        let modifies = batch.split_off(added);
        let adds = batch;

        // Removes first (their rows are the OLD pane's), then adds, then modifies.
        // `remove_entries_by_paths` resolves every entry against the pre-removal
        // listing and drops them highest-index-first, so no removal shifts the next
        // one's entry. Every patch lands in the cache; only the ones the pane shows
        // on some side become a change (`DiffChange::for_pane`), so a dotfile write
        // in `~` with hidden files off emits nothing.
        let _ = self.remove_entries_by_paths(listing_id, &removes);
        for entry in adds {
            let _ = self.insert_entry_sorted(listing_id, entry);
        }
        for mut entry in modifies {
            // A re-stat reads no xattr, so a bare modify would blank the file's
            // Finder tag dots.
            self.carry_forward_tags(listing_id, &mut entry);
            let _ = self.update_entry_sorted(listing_id, entry);
        }
    }
}
