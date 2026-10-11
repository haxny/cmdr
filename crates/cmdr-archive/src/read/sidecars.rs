//! The index's half of AppleDouble sidecars: which entries are sidecars, where
//! each format keeps them, and the xattrs a parse holds or a copy reads. The
//! byte codec is `crate::apple_double`; why and how the index pairs them:
//! `DETAILS.md` § "AppleDouble sidecars".

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cmdr_fs::metadata::Xattr;

use super::index::{EntryStore, MAX_HELD_XATTR_BYTES, RawEntry};
use super::source::ArchiveByteSource;
use crate::apple_double::{self, MacosxEntry};

/// Where a format keeps its AppleDouble sidecars, which the index pairs with
/// their items and hides (`crate::apple_double`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SidecarLayout {
    /// Under the root `__MACOSX/` (a Finder-made zip). Read on demand, per item.
    MacosxFolder,
    /// As a `._name` sibling (a macOS tar), and only when the sibling exists.
    /// Read during the parse ([`RawEntry::sidecar_bytes`]).
    Siblings,
    /// No sidecars (7z): every entry is an item.
    None,
}

/// A zip item's sidecar, read only when something copies the item.
#[derive(Debug, Clone)]
pub(super) struct LazySidecar {
    /// The sidecar entry's own inner path, its read handle's key in the store.
    pub(super) key: String,
    /// The size the archive claims for it, checked against the cap before a read.
    pub(super) size: u64,
}

/// What one entry is to the tree once its name is sanitized.
pub(super) enum EntryRole {
    /// A file, folder, or link the tree shows.
    Item,
    /// The `__MACOSX` sidecar of the item at this path.
    MacosxSidecar(String),
    /// Something else under `__MACOSX`: hidden, and nothing reads it.
    MacosxOther,
    /// A `._name` that's the sidecar of this sibling if the sibling exists, and
    /// an ordinary file if it doesn't.
    SiblingCandidate(String),
}

/// The xattrs an index holds from its parse, capped by
/// [`MAX_HELD_XATTR_BYTES`].
// DEFAULT-OK: an empty holding is an archive whose parse met no xattr.
#[derive(Default)]
pub(super) struct HeldXattrs {
    pub(super) by_path: HashMap<String, Vec<Xattr>>,
    bytes: usize,
    pub(super) dropped: usize,
}

impl HeldXattrs {
    /// Adds `xattrs` to `path`'s set; a name already there takes the new value.
    pub(super) fn add(&mut self, path: &str, xattrs: Vec<Xattr>) {
        for xattr in xattrs {
            let size = xattr.name.len() + xattr.value.len();
            if self.bytes.saturating_add(size) > MAX_HELD_XATTR_BYTES {
                self.dropped += 1;
                continue;
            }
            self.bytes += size;
            let set = self.by_path.entry(path.to_string()).or_default();
            match set.iter_mut().find(|held| held.name == xattr.name) {
                Some(held) => held.value = xattr.value,
                None => set.push(xattr),
            }
        }
    }
}

impl LazySidecar {
    /// This sidecar's bytes, or `None` (logged) when it can't be read whole
    /// within the cap.
    pub(super) async fn read(
        &self,
        store: &EntryStore,
        source: Arc<dyn ArchiveByteSource>,
        password: Option<&str>,
    ) -> Option<Vec<u8>> {
        let sidecar = self;
        let cap = apple_double::MAX_SIDECAR_BYTES;
        if sidecar.size > cap {
            log::warn!(
                target: "archive",
                "Skipped the {}-byte sidecar {}: past the {cap}-byte cap, so its item lands without its xattrs",
                sidecar.size,
                sidecar.key
            );
            return None;
        }
        let mut reader = match store.open_read(&sidecar.key, sidecar.size, source, password) {
            Ok(reader) => reader,
            Err(e) => {
                log::debug!(target: "archive", "Couldn't open the sidecar {}: {e:?}", sidecar.key);
                return None;
            }
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = reader.next_chunk().await {
            match chunk {
                // The claimed size is the archive's word; the decoder's output is
                // what's bounded.
                Ok(chunk) if (bytes.len() + chunk.len()) as u64 <= cap => bytes.extend_from_slice(&chunk),
                Ok(_) => {
                    log::warn!(target: "archive", "Skipped the sidecar {}: it inflates past the {cap}-byte cap", sidecar.key);
                    return None;
                }
                Err(e) => {
                    log::debug!(target: "archive", "Couldn't read the sidecar {}: {e:?}", sidecar.key);
                    return None;
                }
            }
        }
        Some(bytes)
    }
}

/// What the sanitized `path` is to the tree, given the format's `layout`.
pub(super) fn entry_role(path: &str, raw: &RawEntry, layout: SidecarLayout) -> EntryRole {
    match layout {
        SidecarLayout::MacosxFolder => match apple_double::classify_macosx(path) {
            MacosxEntry::Outside => EntryRole::Item,
            MacosxEntry::Sidecar(target) if !raw.is_dir && !raw.is_symlink => EntryRole::MacosxSidecar(target),
            MacosxEntry::Sidecar(_) | MacosxEntry::Other => EntryRole::MacosxOther,
        },
        SidecarLayout::Siblings if !raw.is_dir && !raw.is_symlink => {
            apple_double::sibling_sidecar_target(path).map_or(EntryRole::Item, EntryRole::SiblingCandidate)
        }
        SidecarLayout::Siblings | SidecarLayout::None => EntryRole::Item,
    }
}

/// The sibling paths a `._name` candidate shadows that the archive really holds,
/// as an item or as a folder a deeper item implies, given every accepted
/// `(path, role)`.
pub(super) fn present_siblings<'a>(entries: impl Iterator<Item = (&'a str, &'a EntryRole)> + Clone) -> HashSet<String> {
    let wanted: HashSet<&str> = entries
        .clone()
        .filter_map(|(_, role)| match role {
            EntryRole::SiblingCandidate(target) => Some(target.as_str()),
            _ => None,
        })
        .collect();
    let mut present = HashSet::new();
    if wanted.is_empty() {
        return present;
    }
    for (path, role) in entries {
        if !matches!(role, EntryRole::Item) {
            continue;
        }
        let mut prefix = path;
        loop {
            if wanted.contains(prefix) {
                present.insert(prefix.to_string());
            }
            match prefix.rsplit_once('/') {
                Some((parent, _)) => prefix = parent,
                None => break,
            }
        }
    }
    present
}
