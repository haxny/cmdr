//! What a scan found: one side's tree as comparable entries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::{EntryFacts, EntryKey, EntryKind, SyncCaps};

/// What lies under a folder, counted by the scanner without a second listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubtreeFacts {
    /// Files and symlinks beneath it, however deep.
    pub files: u64,
    /// Their sizes added up (an unknown size counts as zero).
    pub bytes: u64,
    /// The newest modification time of any of them.
    pub newest_mtime: Option<u64>,
    /// Items beneath it the scan left out on purpose (excluded, or hidden while
    /// hidden entries are off). Excluded folders count once, not by contents.
    pub held_back: u64,
}

/// One listed item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// How it pairs with the other side.
    pub key: EntryKey,
    /// Its path from the sync root with `/` between components, in this side's
    /// own spelling. Operations use this, never the key.
    pub rel_path: String,
    /// What the listing said about it.
    pub facts: EntryFacts,
    /// Present for folders, absent for everything else.
    pub subtree: Option<SubtreeFacts>,
}

/// One side of a comparison, as the scan found it.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The folder that was scanned.
    pub root: PathBuf,
    /// What the volume it sits on can do.
    pub caps: SyncCaps,
    /// Every admitted item, parents before children.
    pub entries: BTreeMap<EntryKey, Entry>,
    /// Names that fold to one key within a folder: all spellings, and none of
    /// them in `entries`.
    pub clashes: BTreeMap<EntryKey, Vec<Entry>>,
    /// Folders whose listing failed. Nothing beneath them is known, so nothing
    /// there may be removed on the strength of this snapshot.
    pub unreadable: BTreeSet<EntryKey>,
}

impl Snapshot {
    /// Nothing was found, not even a clash.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.clashes.is_empty()
    }

    /// Files and symlinks among the entries.
    pub fn file_count(&self) -> u64 {
        self.entries
            .values()
            .filter(|entry| entry.facts.kind != EntryKind::Folder)
            .count() as u64
    }
}
