//! What a scan found: one side's tree as comparable entries.

use std::collections::BTreeMap;
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
    /// Unreadable folders beneath it, itself included when it is one. Nothing
    /// about its contents is known.
    ///
    /// ❗ The planner must treat a folder with `unreadable > 0` (or
    /// `held_back > 0`, or `app_owned > 0`) as non-atomic: never copy or remove it as a whole on
    /// partial knowledge.
    pub unreadable: u64,
    /// App-owned `.cmdr-sync` folders the scan skipped beneath it (another
    /// pair's version history). Never in `entries`, never counted as held back,
    /// but a folder with any is non-atomic too: copying or trashing it whole
    /// would carry them along.
    pub app_owned: u64,
    /// An order-independent digest of every admitted name beneath it, with
    /// each file's kind, size, and time and each folder's own digest. Two
    /// counts that agree can still hide a swap or a rename; this can't. Only
    /// comparable within one process (`subtree_now` against the scan).
    pub digest: u64,
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

/// Why a folder wouldn't list, from the volume's typed error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableReason {
    /// The volume refused access.
    PermissionDenied,
    /// The folder is gone.
    NotFound,
    /// The device or server timed out, dropped, or isn't connected.
    Unreachable,
    /// Anything else.
    Other,
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
    /// Folders whose listing failed, with why. Nothing beneath them is known, so
    /// nothing there may be removed on the strength of this snapshot.
    pub unreadable: BTreeMap<EntryKey, UnreadableReason>,
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
