//! The walk: one listing per folder over any `Volume`, into a `Snapshot`.
//!
//! Never stats a child (the listing's facts are the facts), prunes excluded
//! folders before listing them, and keeps going past a subfolder that won't list.

use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ScanBoundary, ScanListingClock, ScanStop, ScanStopSignal, Volume, VolumeError};

use crate::filter::is_app_owned;
use crate::{
    Admission, Entry, EntryFacts, EntryKey, EntryKind, MaskFilter, NameRule, Snapshot, SubtreeFacts, SyncCaps,
    SyncControl, UnreadableReason,
};

/// Which pane a scan belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    /// The left folder.
    Left,
    /// The right folder.
    Right,
}

/// What to walk and what to leave out.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Descend into folders. Off: only the root's files are listed, and folders
    /// are not entries at all.
    pub subfolders: bool,
    /// Take hidden entries along. Off: they count as held back.
    pub include_hidden: bool,
    /// Keep folders that end up with nothing admitted inside.
    pub empty_folders: bool,
    /// The include/exclude masks.
    pub filter: MaskFilter,
    /// How names pair across the two sides.
    pub name_rule: NameRule,
    /// Restrict the top level to these names (the selection), if set.
    pub only: Option<Vec<String>>,
}

/// Running totals, reported after each folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgress {
    /// Which scan this is.
    pub side: Side,
    /// Folders listed so far.
    pub folders: u64,
    /// Files and symlinks admitted so far.
    pub files: u64,
    /// Their sizes added up.
    pub bytes: u64,
}

/// Why a scan gave no snapshot.
#[derive(Debug)]
pub enum ScanError {
    /// The root itself wouldn't list: unmounted, gone, or refused.
    RootUnreachable {
        /// Which side.
        side: Side,
        /// What the volume said.
        error: VolumeError,
    },
    /// Cancelled.
    Stopped,
}

enum FolderError {
    Stopped,
    Listing(VolumeError),
}

struct Folder {
    facts: SubtreeFacts,
    /// How many items were admitted directly inside (an empty folder has none).
    kept: u64,
}

/// Walks `root` on `volume`. Cancel and pause go through `control`.
pub async fn scan(
    side: Side,
    volume: &dyn Volume,
    root: &Path,
    options: &ScanOptions,
    control: &Arc<SyncControl>,
    on_progress: &(dyn Fn(ScanProgress) + Sync),
) -> Result<Snapshot, ScanError> {
    let signal: Arc<dyn ScanStopSignal> = Arc::clone(control) as Arc<dyn ScanStopSignal>;
    let mut walk = Walk {
        volume,
        options,
        control,
        boundary: ScanBoundary::new(None).stopping_at(ScanStop::new(signal)),
        on_progress,
        only: options
            .only
            .as_ref()
            .map(|names| names.iter().map(|name| options.name_rule.key(name)).collect()),
        progress: ScanProgress {
            side,
            folders: 0,
            files: 0,
            bytes: 0,
        },
        entries: BTreeMap::new(),
        clashes: BTreeMap::new(),
        unreadable: BTreeMap::new(),
    };
    match walk.folder(root, "", &EntryKey::root()).await {
        Ok(_) => {}
        Err(FolderError::Stopped) => return Err(ScanError::Stopped),
        Err(FolderError::Listing(error)) => return Err(ScanError::RootUnreachable { side, error }),
    }
    Ok(Snapshot {
        root: root.to_path_buf(),
        caps: SyncCaps::of(volume),
        entries: walk.entries,
        clashes: walk.clashes,
        unreadable: walk.unreadable,
    })
}

/// What lies under the folder at `path` (`rel` from the sync root, `/`-separated)
/// now, counted exactly as `scan` with the same `options` counted it, digest
/// included. The executor's re-check before acting on a folder as a whole.
pub async fn subtree_now(
    side: Side,
    volume: &dyn Volume,
    path: &Path,
    rel: &str,
    options: &ScanOptions,
    control: &Arc<SyncControl>,
) -> Result<SubtreeFacts, ScanError> {
    let signal: Arc<dyn ScanStopSignal> = Arc::clone(control) as Arc<dyn ScanStopSignal>;
    let key = rel
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(EntryKey::root(), |key, part| key.child(options.name_rule.key(part)));
    let mut walk = Walk {
        volume,
        options,
        control,
        boundary: ScanBoundary::new(None).stopping_at(ScanStop::new(signal)),
        on_progress: &|_| {},
        only: None,
        progress: ScanProgress {
            side,
            folders: 0,
            files: 0,
            bytes: 0,
        },
        entries: BTreeMap::new(),
        clashes: BTreeMap::new(),
        unreadable: BTreeMap::new(),
    };
    match walk.folder(path, rel, &key).await {
        Ok(folder) => Ok(folder.facts),
        Err(FolderError::Stopped) => Err(ScanError::Stopped),
        Err(FolderError::Listing(error)) => Err(ScanError::RootUnreachable { side, error }),
    }
}

/// One child's share of its folder's digest. A folder brings its own digest
/// (`None` marks one that wouldn't list), so a change anywhere below shows.
fn child_digest(name: &str, facts: &EntryFacts, inner: Option<Option<u64>>) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    match inner {
        Some(folder) => {
            EntryKind::Folder.hash(&mut hasher);
            folder.hash(&mut hasher);
        }
        None => {
            facts.kind.hash(&mut hasher);
            facts.size.hash(&mut hasher);
            facts.mtime.hash(&mut hasher);
        }
    }
    hasher.finish()
}

struct Walk<'a> {
    volume: &'a dyn Volume,
    options: &'a ScanOptions,
    control: &'a Arc<SyncControl>,
    boundary: ScanBoundary<'a>,
    on_progress: &'a (dyn Fn(ScanProgress) + Sync),
    only: Option<HashSet<String>>,
    progress: ScanProgress,
    entries: BTreeMap<EntryKey, Entry>,
    clashes: BTreeMap<EntryKey, Vec<Entry>>,
    unreadable: BTreeMap<EntryKey, UnreadableReason>,
}

type Candidates = BTreeMap<String, Vec<(FileEntry, String)>>;

impl<'a> Walk<'a> {
    fn folder<'w>(
        &'w mut self,
        path: &'w Path,
        rel: &'w str,
        key: &'w EntryKey,
    ) -> Pin<Box<dyn Future<Output = Result<Folder, FolderError>> + 'w>> {
        Box::pin(async move {
            // Before the round trip, which is the slow part.
            self.boundary.dir(path).await.map_err(|_| FolderError::Stopped)?;
            let clock = ScanListingClock::start();
            let listing = self
                .volume
                .list_directory_for_scan(path, Some(self.control.token()), &clock)
                .await;
            let children = match listing {
                Ok(children) => children,
                Err(VolumeError::Cancelled(_)) => return Err(FolderError::Stopped),
                Err(_) if self.control.is_cancelled() => return Err(FolderError::Stopped),
                Err(error) => return Err(FolderError::Listing(error)),
            };
            self.progress.folders += 1;
            let mut facts = SubtreeFacts::default();
            let groups = self.admit(children, rel, key, &mut facts);
            let kept = self.keep(groups, key, &mut facts).await?;
            Ok(Folder { facts, kept })
        })
    }

    /// Filters one listing and groups the survivors by name key.
    fn admit(&self, children: Vec<FileEntry>, rel: &str, key: &EntryKey, facts: &mut SubtreeFacts) -> Candidates {
        let options = self.options;
        let top = key == &EntryKey::root();
        let mut groups = Candidates::new();
        for child in children {
            if is_app_owned(&child.name) {
                facts.app_owned += 1;
                continue;
            }
            let name_key = options.name_rule.key(&child.name);
            if top && self.only.as_ref().is_some_and(|only| !only.contains(&name_key)) {
                continue;
            }
            if child.is_directory && !child.is_symlink && !options.subfolders {
                continue;
            }
            let rel_child = if rel.is_empty() {
                child.name.clone()
            } else {
                format!("{rel}/{}", child.name)
            };
            let is_folder = child.is_directory && !child.is_symlink;
            match options.filter.admits(&rel_child, is_folder) {
                Admission::Out => {
                    facts.held_back += 1;
                    continue;
                }
                Admission::Junk => continue,
                Admission::In => {}
            }
            if child.is_hidden && !options.include_hidden {
                facts.held_back += 1;
                continue;
            }
            groups.entry(name_key).or_default().push((child, rel_child));
        }
        groups
    }

    /// Turns the survivors into entries, files first, then folders (recursing).
    async fn keep(&mut self, groups: Candidates, key: &EntryKey, facts: &mut SubtreeFacts) -> Result<u64, FolderError> {
        let mut kept = 0;
        let mut folders = Vec::new();
        for (name_key, mut group) in groups {
            let child_key = key.child(name_key);
            if group.len() > 1 {
                let twins: Vec<Entry> = group
                    .iter()
                    .map(|(child, rel)| make_entry(&child_key, rel, child, None))
                    .collect();
                for (twin, (child, _)) in twins.iter().zip(&group) {
                    facts.digest = facts.digest.wrapping_add(child_digest(&child.name, &twin.facts, None));
                }
                self.clashes.insert(child_key, twins);
                kept += 1;
                continue;
            }
            let (child, rel_child) = group.remove(0);
            if child.is_directory && !child.is_symlink {
                folders.push((child, rel_child, child_key));
                continue;
            }
            let size = child.size.unwrap_or(0);
            self.boundary
                .file(Path::new(&child.path), size)
                .await
                .map_err(|_| FolderError::Stopped)?;
            facts.files += 1;
            facts.bytes += size;
            facts.newest_mtime = facts.newest_mtime.max(child.modified_at);
            facts.digest = facts
                .digest
                .wrapping_add(child_digest(&child.name, &EntryFacts::of(&child), None));
            self.progress.files += 1;
            self.progress.bytes += size;
            self.entries
                .insert(child_key.clone(), make_entry(&child_key, &rel_child, &child, None));
            kept += 1;
        }
        (self.on_progress)(self.progress);
        for (child, rel_child, child_key) in folders {
            let subtree = match self.folder(Path::new(&child.path), &rel_child, &child_key).await {
                Ok(inner) => {
                    add(facts, &inner.facts);
                    let own = child_digest(&child.name, &EntryFacts::of(&child), Some(Some(inner.facts.digest)));
                    facts.digest = facts.digest.wrapping_add(own);
                    if inner.kept == 0 && !self.options.empty_folders {
                        continue;
                    }
                    inner.facts
                }
                Err(FolderError::Stopped) => return Err(FolderError::Stopped),
                Err(FolderError::Listing(error)) => {
                    self.unreadable.insert(child_key.clone(), reason_of(&error));
                    let unreadable = SubtreeFacts {
                        unreadable: 1,
                        ..SubtreeFacts::default()
                    };
                    add(facts, &unreadable);
                    facts.digest =
                        facts
                            .digest
                            .wrapping_add(child_digest(&child.name, &EntryFacts::of(&child), Some(None)));
                    unreadable
                }
            };
            self.entries.insert(
                child_key.clone(),
                make_entry(&child_key, &rel_child, &child, Some(subtree)),
            );
            kept += 1;
        }
        Ok(kept)
    }
}

fn make_entry(key: &EntryKey, rel_path: &str, child: &FileEntry, subtree: Option<SubtreeFacts>) -> Entry {
    Entry {
        key: key.clone(),
        rel_path: rel_path.to_string(),
        facts: EntryFacts::of(child),
        subtree,
    }
}

fn add(total: &mut SubtreeFacts, inner: &SubtreeFacts) {
    total.files += inner.files;
    total.bytes += inner.bytes;
    total.newest_mtime = total.newest_mtime.max(inner.newest_mtime);
    total.held_back += inner.held_back;
    total.unreadable += inner.unreadable;
    total.app_owned += inner.app_owned;
}

fn reason_of(error: &VolumeError) -> UnreadableReason {
    match error {
        VolumeError::PermissionDenied { .. } => UnreadableReason::PermissionDenied,
        VolumeError::NotFound(_) => UnreadableReason::NotFound,
        VolumeError::ConnectionTimeout(_)
        | VolumeError::DeviceDisconnected(_)
        | VolumeError::DeviceSessionReset(_)
        | VolumeError::NotConnected(_) => UnreadableReason::Unreachable,
        _ => UnreadableReason::Other,
    }
}
