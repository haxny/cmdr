//! [`ResticRepos`]: the one hub every restic view shares. It holds what's
//! costly to get again (passwords, opened repos, loaded indexes, the snapshot
//! lists) and the memo of which folders are repos, so the volumes over it stay
//! cheap and a routing cache dropping one loses nothing.
//!
//! A value the host parks once, ❌ never a static: a test builds its own.

use crate::detect::is_restic_repo;
use crate::path::VIRTUAL_ENTRY_NAME;
use crate::repo::{IndexedRepo, ResticRepo};
use crate::snapshots::{Snapshot, SnapshotList, snapshot_list_of};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{Volume, VolumeError};
use rustic_core::repofile::SnapshotFile;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// The timings the hub runs on. Tests shorten them; the host never does.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tuning {
    /// How long nothing may have read under `latest/` before a root refresh
    /// moves `latest` to a newer snapshot.
    pub(crate) latest_quiet: Duration,
    /// How long a "this folder isn't a repo" answer is trusted.
    pub(crate) not_a_repo_for: Duration,
    /// How long confirming a folder may take before it counts as unconfirmed
    /// (and isn't remembered): a wedged share would otherwise hang every path
    /// below it.
    pub(crate) confirm_deadline: Duration,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            latest_quiet: Duration::from_secs(30),
            not_a_repo_for: Duration::from_secs(30),
            confirm_deadline: Duration::from_secs(5),
        }
    }
}

/// The most confirmation answers the memo keeps. Far above the repos anyone
/// browses; it bounds a walk that meets thousands of folders named
/// [`VIRTUAL_ENTRY_NAME`].
const MAX_CONFIRMATIONS: usize = 1_024;

/// One repo: the volume it's on, by id, and its folder there, spelled as the
/// path spelled it (❌ no `canonicalize`: that's a `stat`, and on a network
/// parent a slow one).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RepoKey {
    pub(crate) volume_id: String,
    pub(crate) root: PathBuf,
}

/// What the memo knows about a folder.
#[derive(Debug, Clone, Copy)]
enum Confirmation {
    Repo,
    NotARepo { since: Instant },
}

/// The hub of every restic repo the app has met: passwords, opened repos,
/// loaded indexes, snapshot lists, and which folders are repos at all.
///
/// The host builds one and shares it (`Arc`) with every `ResticVolume`. The
/// heavy state lives here so a routing cache evicting a cheap volume never
/// drops an index or forgets a password.
pub struct ResticRepos {
    host: VolumeHost,
    tuning: Tuning,
    repos: Mutex<HashMap<RepoKey, Arc<RepoSlot>>>,
    confirmations: Mutex<HashMap<RepoKey, Confirmation>>,
}

impl ResticRepos {
    /// A hub with nothing unlocked and nothing confirmed. Its rustic work runs
    /// on `host`'s runtime.
    pub fn new(host: VolumeHost) -> Self {
        Self::with_tuning(host, Tuning::default())
    }

    pub(crate) fn with_tuning(host: VolumeHost, tuning: Tuning) -> Self {
        Self {
            host,
            tuning,
            repos: Mutex::new(HashMap::new()),
            confirmations: Mutex::new(HashMap::new()),
        }
    }

    /// Whether `repo_root` on `parent` (registered as `volume_id`) is a restic
    /// repo: the check a host runs before routing a path through the view.
    ///
    /// Answered from the memo when it can. A miss costs ONE listing of
    /// `repo_root`, bounded by a deadline; a yes is remembered for good, a no for
    /// a short while (so a real folder named [`VIRTUAL_ENTRY_NAME`] outside a
    /// repo doesn't pay a listing per call), and a timeout or a failing parent
    /// not at all.
    pub async fn confirm_repo(&self, volume_id: &str, parent: &dyn Volume, repo_root: &Path) -> bool {
        let key = key(volume_id, repo_root);
        if let Some(known) = self.remembered(&key) {
            return known;
        }
        let listing = tokio::time::timeout(self.tuning.confirm_deadline, parent.list_directory(repo_root, None)).await;
        let answer = match listing {
            Ok(Ok(entries)) => is_restic_repo(&entries),
            Ok(Err(VolumeError::NotFound(_) | VolumeError::NotADirectory(_))) => false,
            // Wedged or failing: no verdict, so nothing to remember.
            Ok(Err(_)) | Err(_) => return false,
        };
        self.remember(key, answer);
        answer
    }

    /// What the memo knows about `repo_root` on `volume_id`, with zero I/O:
    /// `Some(true)` confirmed, `Some(false)` recently found not to be a repo,
    /// `None` never asked (or the "no" went stale). For a synchronous seam that
    /// can't afford a listing.
    pub fn serves_confirmed(&self, volume_id: &str, repo_root: &Path) -> Option<bool> {
        self.remembered(&key(volume_id, repo_root))
    }

    /// The [`VIRTUAL_ENTRY_NAME`] row a pane's listing of `repo_root` gains, once
    /// the host saw from that listing's entries that it's a repo
    /// (`is_restic_repo`). Also confirms `repo_root` in the memo, for free, so
    /// entering the row costs no confirmation listing.
    pub fn overlay_row(&self, volume_id: &str, repo_root: &Path) -> FileEntry {
        self.remember(key(volume_id, repo_root), true);
        let path = repo_root.join(VIRTUAL_ENTRY_NAME);
        FileEntry {
            extended_metadata_loaded: true,
            ..FileEntry::new(
                VIRTUAL_ENTRY_NAME.to_string(),
                path.to_string_lossy().into_owned(),
                true,
                false,
            )
        }
    }

    fn remembered(&self, key: &RepoKey) -> Option<bool> {
        match lock(&self.confirmations).get(key)? {
            Confirmation::Repo => Some(true),
            Confirmation::NotARepo { since } if since.elapsed() < self.tuning.not_a_repo_for => Some(false),
            Confirmation::NotARepo { .. } => None,
        }
    }

    fn remember(&self, key: RepoKey, is_repo: bool) {
        let mut memo = lock(&self.confirmations);
        if memo.len() >= MAX_CONFIRMATIONS && !memo.contains_key(&key) {
            // Stale and recent "no"s go first; a confirmed repo is worth keeping.
            memo.retain(|_, known| matches!(known, Confirmation::Repo));
        }
        let known = if is_repo {
            Confirmation::Repo
        } else {
            Confirmation::NotARepo { since: Instant::now() }
        };
        memo.insert(key, known);
    }

    pub(crate) fn host(&self) -> &VolumeHost {
        &self.host
    }

    /// The slot for one repo, made on first use. A slot is never dropped, so the
    /// password in it outlives every volume over it.
    pub(crate) fn slot(&self, volume_id: &str, repo_root: &Path) -> Arc<RepoSlot> {
        let key = key(volume_id, repo_root);
        let mut repos = lock(&self.repos);
        Arc::clone(
            repos
                .entry(key)
                .or_insert_with_key(|key| Arc::new(RepoSlot::new(key.root.clone()))),
        )
    }

    /// Tries `password` on the repo in `slot`, through `parent`: `Ok(true)` when
    /// it unlocks (the password and the opened repo are kept), `Ok(false)` when
    /// it doesn't (nothing is kept). `Err` when the repo couldn't be asked.
    ///
    /// Commits only after the open returns, so a caller that drops this future
    /// mid-scrypt leaves no trace.
    ///
    /// Reached only through `test_fixtures::unlock` until a `PasswordLocked` seam
    /// calls it, hence the gate.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) async fn unlock(
        &self,
        slot: &RepoSlot,
        parent: &Arc<dyn Volume>,
        password: &str,
    ) -> Result<bool, VolumeError> {
        let opened = ResticRepo::open(Arc::clone(parent), slot.root.clone(), password, None, &self.host).await;
        match opened {
            Ok(repo) => {
                *lock(&slot.password) = Some(Zeroizing::new(password.to_string()));
                *slot.opened.lock().await = Some(Bound::new(parent, Arc::new(repo)));
                Ok(true)
            }
            Err(crate::repo::ResticError::WrongPassword) => Ok(false),
            Err(err) => Err(err.into_volume_error(&slot.root)),
        }
    }

    /// The repo opened for the snapshot list, through `parent`: the one already
    /// open, or a fresh open with the slot's password when there's none or it
    /// was opened through a parent since replaced.
    async fn opened(&self, slot: &RepoSlot, parent: &Arc<dyn Volume>) -> Result<Arc<ResticRepo>, VolumeError> {
        let mut opened = slot.opened.lock().await;
        if let Some(bound) = opened.as_ref().filter(|bound| bound.is_over(parent)) {
            return Ok(Arc::clone(&bound.value));
        }
        let password = slot.password()?;
        let repo = ResticRepo::open(Arc::clone(parent), slot.root.clone(), &password, None, &self.host)
            .await
            .map_err(|err| err.into_volume_error(&slot.root))?;
        let repo = Arc::new(repo);
        *opened = Some(Bound::new(parent, Arc::clone(&repo)));
        Ok(repo)
    }

    /// The repo's snapshots. `refresh` reads the repo's `snapshots/` again
    /// (only the new files), which every listing of the view's root does so a
    /// manual refresh shows a new backup; otherwise the list read last is
    /// reused, and read only when there's none yet.
    ///
    /// A refresh is also the one moment `latest` may move: to the newest
    /// snapshot, once nothing has read under `latest/` for the quiet period, or
    /// at once when the snapshot it showed is gone from the repo.
    pub(crate) async fn snapshots(
        &self,
        slot: &RepoSlot,
        parent: &Arc<dyn Volume>,
        refresh: bool,
    ) -> Result<Arc<SnapshotList>, VolumeError> {
        let known = lock(&slot.snapshots).clone();
        if let Some(known) = known.as_ref().filter(|_| !refresh) {
            return Ok(Arc::clone(&known.list));
        }
        let repo = self.opened(slot, parent).await?;
        let current = known.map(|known| known.files).unwrap_or_default();
        let files = repo
            .refresh_snapshots(current)
            .await
            .map_err(|err| err.into_volume_error(&slot.root))?;
        let list = Arc::new(snapshot_list_of(&files));
        lock(&slot.latest).rebind(&list, self.tuning.latest_quiet);
        *lock(&slot.snapshots) = Some(Snapshots {
            files,
            list: Arc::clone(&list),
        });
        Ok(list)
    }

    /// The repo opened and fully indexed, ready to browse `snapshot`'s trees and
    /// read its files. Loaded on first use, and again when `snapshot` is newer
    /// than the loaded index (its trees are in index files the load never read).
    ///
    /// One load at a time per repo: a second caller waits for the first.
    pub(crate) async fn indexed(
        &self,
        slot: &RepoSlot,
        parent: &Arc<dyn Volume>,
        snapshot: &Snapshot,
    ) -> Result<Arc<IndexedRepo>, VolumeError> {
        let mut indexed = slot.indexed.lock().await;
        if let Some(bound) = indexed
            .as_ref()
            .filter(|bound| bound.is_over(parent) && bound.value.covers.contains(&snapshot.id))
        {
            return Ok(Arc::clone(&bound.value.repo));
        }
        // Restic writes a backup's index files before its snapshot file, so
        // every snapshot listed now has its trees in the index read next.
        let covers = lock(&slot.snapshots)
            .as_ref()
            .map(|known| known.list.snapshots().iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default();
        let password = slot.password()?;
        let repo = IndexedRepo::open(Arc::clone(parent), slot.root.clone(), &password, None, &self.host)
            .await
            .map_err(|err| err.into_volume_error(&slot.root))?;
        let repo = Arc::new(repo);
        *indexed = Some(Bound::new(
            parent,
            Indexed {
                repo: Arc::clone(&repo),
                covers,
            },
        ));
        Ok(repo)
    }

    /// How many writes rustic tried through the repos open right now (every one
    /// refused).
    #[cfg(test)]
    pub(crate) fn write_attempts(&self) -> usize {
        let slots: Vec<Arc<RepoSlot>> = lock(&self.repos).values().cloned().collect();
        slots
            .iter()
            .map(|slot| {
                let opened = slot
                    .opened
                    .try_lock()
                    .ok()
                    .and_then(|o| o.as_ref().map(|b| b.value.backend.write_attempts()));
                let indexed = slot
                    .indexed
                    .try_lock()
                    .ok()
                    .and_then(|i| i.as_ref().map(|b| b.value.repo.backend.write_attempts()));
                opened.unwrap_or(0) + indexed.unwrap_or(0)
            })
            .sum()
    }
}

fn key(volume_id: &str, repo_root: &Path) -> RepoKey {
    RepoKey {
        volume_id: volume_id.to_string(),
        root: repo_root.to_path_buf(),
    }
}

/// Locks a std mutex, taking the data over from a thread that panicked holding
/// it: every value here stays whole between statements.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything the hub keeps about one repo.
///
/// The two async mutexes are per repo, ❌ never the hub's: holding one across an
/// open or an index load keeps a second caller for the SAME repo from starting
/// another, and no other repo waits on it.
pub(crate) struct RepoSlot {
    root: PathBuf,
    /// The password that last unlocked it. Wiped on drop.
    password: Mutex<Option<Zeroizing<String>>>,
    opened: tokio::sync::Mutex<Option<Bound<Arc<ResticRepo>>>>,
    indexed: tokio::sync::Mutex<Option<Bound<Indexed>>>,
    snapshots: Mutex<Option<Snapshots>>,
    latest: Mutex<LatestPin>,
}

impl RepoSlot {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            password: Mutex::new(None),
            opened: tokio::sync::Mutex::new(None),
            indexed: tokio::sync::Mutex::new(None),
            snapshots: Mutex::new(None),
            latest: Mutex::new(LatestPin::default()),
        }
    }

    /// A copy of the password, or `NeedsPassword` when nothing unlocked it yet.
    fn password(&self) -> Result<Zeroizing<String>, VolumeError> {
        lock(&self.password)
            .clone()
            .ok_or(VolumeError::NeedsPassword { wrong_attempt: false })
    }

    /// The snapshot the `latest` folder shows, in `list`, for a path under
    /// `latest/`: a read there, which holds `latest` in place.
    pub(crate) fn latest_in<'l>(&self, list: &'l SnapshotList) -> Option<&'l Snapshot> {
        self.touch_latest();
        self.latest_shown(list)
    }

    /// The snapshot the `latest` row shows, in `list`, without counting as a
    /// read: listing the root only shows `latest`, and a person refreshing to
    /// see a new backup must see it move.
    pub(crate) fn latest_shown<'l>(&self, list: &'l SnapshotList) -> Option<&'l Snapshot> {
        list.by_id(lock(&self.latest).id.as_deref()?)
    }

    /// Notes that something read under `latest/` just now, which holds it in
    /// place for the quiet period.
    pub(crate) fn touch_latest(&self) {
        lock(&self.latest).touch();
    }
}

/// A value opened through one particular parent volume. A parent can be
/// replaced under the same volume id (an SMB share reconnecting), and a value
/// opened through the old one would read through a retired volume forever.
struct Bound<T> {
    parent: Arc<dyn Volume>,
    value: T,
}

impl<T> Bound<T> {
    fn new(parent: &Arc<dyn Volume>, value: T) -> Self {
        Self {
            parent: Arc::clone(parent),
            value,
        }
    }

    fn is_over(&self, parent: &Arc<dyn Volume>) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.parent), Arc::as_ptr(parent))
    }
}

/// A loaded index, and the snapshots whose trees it holds for sure.
struct Indexed {
    repo: Arc<IndexedRepo>,
    covers: HashSet<String>,
}

/// The snapshot list read last: rustic's files (handed back on a refresh, so
/// only new ones are read) and the named list built from them.
#[derive(Clone)]
struct Snapshots {
    files: Vec<SnapshotFile>,
    list: Arc<SnapshotList>,
}

/// Which snapshot `latest` shows, and when something last read under it.
#[derive(Default)]
struct LatestPin {
    id: Option<String>,
    touched: Option<Instant>,
}

impl LatestPin {
    fn touch(&mut self) {
        self.touched = Some(Instant::now());
    }

    /// After a refresh: moves to `list`'s newest snapshot when nothing has
    /// pinned it, nothing read under it for `quiet`, or its snapshot is gone.
    fn rebind(&mut self, list: &SnapshotList, quiet: Duration) {
        let newest = list.latest().map(|s| s.id.clone());
        let idle = self.touched.is_none_or(|at| at.elapsed() >= quiet);
        let gone = self.id.as_deref().is_none_or(|id| list.by_id(id).is_none());
        if idle || gone {
            self.id = newest;
        }
    }
}

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;
