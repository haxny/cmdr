//! Snapshot builders for the planner, guard, and view tests (test-only).
//!
//! Paths are `/`-separated; keys are the path components as given (tests use
//! lowercase names, so the key and the spelling agree). Lenient on purpose, so
//! the property tests can feed it random operations: anything that contradicts
//! what is already there (a file under a file, a write under a clash or an
//! unreadable folder) is ignored.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{Entry, EntryFacts, EntryKey, EntryKind, Snapshot, SubtreeFacts, SyncCaps, UnreadableReason};

/// A writable local-like volume.
pub(crate) fn caps() -> SyncCaps {
    SyncCaps {
        case_sensitive: false,
        names_in_any_unicode_form: true,
        mtime_precision_secs: 1,
        can_write: true,
        has_trash: true,
        atomic_replace: true,
        can_read_content: true,
        renames_can_copy: false,
    }
}

/// The key of a `/`-separated path.
pub(crate) fn key(path: &str) -> EntryKey {
    EntryKey::Name(
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

fn rel_path(key: &EntryKey) -> String {
    let EntryKey::Name(parts) = key;
    parts.join("/")
}

fn spelled(key: &EntryKey, upper: bool) -> String {
    let path = rel_path(key);
    let (dir, name) = path.rsplit_once('/').map_or(("", path.as_str()), |(d, n)| (d, n));
    let name = if upper { name.to_uppercase() } else { name.to_owned() };
    if dir.is_empty() { name } else { format!("{dir}/{name}") }
}

/// One side under construction.
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    caps: SyncCaps,
    entries: BTreeMap<EntryKey, Entry>,
    own_held_back: BTreeMap<EntryKey, u64>,
    own_app_owned: BTreeMap<EntryKey, u64>,
    clashes: BTreeMap<EntryKey, Vec<Entry>>,
    unreadable: BTreeMap<EntryKey, UnreadableReason>,
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

impl Tree {
    pub(crate) fn new() -> Self {
        Self {
            caps: caps(),
            entries: BTreeMap::new(),
            own_held_back: BTreeMap::new(),
            own_app_owned: BTreeMap::new(),
            clashes: BTreeMap::new(),
            unreadable: BTreeMap::new(),
        }
    }

    pub(crate) fn read_only(mut self) -> Self {
        self.caps.can_write = false;
        self
    }

    pub(crate) fn with_caps(mut self, caps: SyncCaps) -> Self {
        self.caps = caps;
        self
    }

    /// A regular file with a known size and time.
    pub(crate) fn file(self, path: &str, size: u64, mtime: u64) -> Self {
        self.entry(path, EntryKind::File, Some(size), Some(mtime))
    }

    /// Any non-folder entry with the facts as given.
    pub(crate) fn entry(mut self, path: &str, kind: EntryKind, size: Option<u64>, mtime: Option<u64>) -> Self {
        let key = key(path);
        if !self.can_place(&key)
            || self
                .entries
                .get(&key)
                .is_some_and(|e| e.facts.kind == EntryKind::Folder)
        {
            return self;
        }
        self.ensure_parents(&key);
        let facts = EntryFacts { kind, size, mtime };
        let entry = Entry {
            key: key.clone(),
            rel_path: rel_path(&key),
            facts,
            subtree: None,
        };
        self.entries.insert(key, entry);
        self
    }

    /// A folder (and its parents).
    pub(crate) fn folder(mut self, path: &str) -> Self {
        let key = key(path);
        if !self.can_place(&key)
            || self
                .entries
                .get(&key)
                .is_some_and(|e| e.facts.kind != EntryKind::Folder)
        {
            return self;
        }
        self.ensure_parents(&key);
        self.insert_folder(key);
        self
    }

    /// `count` items the scan left out of folder `path` (made a folder if new).
    pub(crate) fn held_back(self, path: &str, count: u64) -> Self {
        let tree = self.folder(path);
        let key = key(path);
        if tree.entries.get(&key).is_none_or(|e| e.facts.kind != EntryKind::Folder) {
            return tree;
        }
        let mut tree = tree;
        *tree.own_held_back.entry(key).or_insert(0) += count;
        tree
    }

    /// A `.cmdr-sync` folder the scan skipped inside folder `path` (made a
    /// folder if new).
    pub(crate) fn app_owned(self, path: &str) -> Self {
        let tree = self.folder(path);
        let key = key(path);
        if tree.entries.get(&key).is_none_or(|e| e.facts.kind != EntryKind::Folder) {
            return tree;
        }
        let mut tree = tree;
        *tree.own_app_owned.entry(key).or_insert(0) += 1;
        tree
    }

    /// A folder that wouldn't list: an entry, nothing beneath it.
    pub(crate) fn unreadable(self, path: &str) -> Self {
        let mut tree = self.folder(path);
        let key = key(path);
        if tree.entries.get(&key).is_none_or(|e| e.facts.kind != EntryKind::Folder) {
            return tree;
        }
        tree.drop_beneath(&key);
        tree.unreadable.insert(key, UnreadableReason::PermissionDenied);
        tree
    }

    /// Two spellings of one name (`Name` and `name`): out of `entries`, into
    /// `clashes`, with everything that was beneath it gone.
    pub(crate) fn clash(mut self, path: &str) -> Self {
        let key = key(path);
        if !self.can_place(&key) {
            return self;
        }
        self.ensure_parents(&key);
        self.drop_beneath(&key);
        self.entries.remove(&key);
        self.own_held_back.remove(&key);
        self.own_app_owned.remove(&key);
        let twin = |upper| Entry {
            key: key.clone(),
            rel_path: spelled(&key, upper),
            facts: EntryFacts {
                kind: EntryKind::File,
                size: Some(1),
                mtime: Some(1),
            },
            subtree: None,
        };
        self.clashes.insert(key.clone(), vec![twin(true), twin(false)]);
        self
    }

    /// Nothing may be placed at or under a clash or an unreadable folder, nor
    /// under a non-folder.
    fn can_place(&self, key: &EntryKey) -> bool {
        let mut prefix = EntryKey::root();
        let EntryKey::Name(parts) = key;
        for (depth, part) in parts.iter().enumerate() {
            prefix = prefix.child(part.clone());
            if self.clashes.contains_key(&prefix) || self.unreadable.contains_key(&prefix) {
                return false;
            }
            let is_last = depth + 1 == parts.len();
            if !is_last
                && self
                    .entries
                    .get(&prefix)
                    .is_some_and(|e| e.facts.kind != EntryKind::Folder)
            {
                return false;
            }
        }
        true
    }

    fn ensure_parents(&mut self, key: &EntryKey) {
        let mut parent = key.parent();
        while let Some(folder) = parent {
            if folder == EntryKey::root() {
                break;
            }
            parent = folder.parent();
            self.insert_folder(folder);
        }
    }

    fn insert_folder(&mut self, key: EntryKey) {
        self.entries.entry(key.clone()).or_insert_with(|| Entry {
            rel_path: rel_path(&key),
            key,
            facts: EntryFacts {
                kind: EntryKind::Folder,
                size: None,
                mtime: Some(1),
            },
            subtree: Some(SubtreeFacts::default()),
        });
    }

    fn drop_beneath(&mut self, key: &EntryKey) {
        self.entries.retain(|k, _| k == key || !k.is_within(key));
        self.own_held_back.retain(|k, _| k == key || !k.is_within(key));
        self.own_app_owned.retain(|k, _| k == key || !k.is_within(key));
        self.clashes.retain(|k, _| k == key || !k.is_within(key));
        self.unreadable.retain(|k, _| k == key || !k.is_within(key));
    }

    /// The snapshot, with every folder's `SubtreeFacts` added up from beneath.
    pub(crate) fn build(self) -> Snapshot {
        let mut entries = self.entries.clone();
        for (folder_key, folder) in &mut entries {
            if folder.facts.kind != EntryKind::Folder {
                continue;
            }
            folder.subtree = Some(self.subtree_of(folder_key));
        }
        Snapshot {
            root: PathBuf::from("/root"),
            caps: self.caps,
            entries,
            clashes: self.clashes,
            unreadable: self.unreadable,
        }
    }

    fn subtree_of(&self, folder: &EntryKey) -> SubtreeFacts {
        let mut facts = SubtreeFacts::default();
        let beneath = |k: &EntryKey| k.is_within(folder);
        for (k, entry) in self.entries.range(folder.clone()..).take_while(|(k, _)| beneath(k)) {
            if entry.facts.kind != EntryKind::Folder {
                facts.files += 1;
                facts.bytes += entry.facts.size.unwrap_or(0);
                facts.newest_mtime = facts.newest_mtime.max(entry.facts.mtime);
            }
            facts.held_back += self.own_held_back.get(k).copied().unwrap_or(0);
            facts.app_owned += self.own_app_owned.get(k).copied().unwrap_or(0);
            facts.unreadable += u64::from(self.unreadable.contains_key(k));
        }
        facts
    }
}
