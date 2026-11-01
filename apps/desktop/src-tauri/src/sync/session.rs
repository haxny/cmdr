//! One Synchronize folders dialog = one session: its compare (snapshots and the
//! plan with the person's changes), the grid's pages, and at most one run.
//! Lifecycle, the run lock, and why the plan lives here: `DETAILS.md` §
//! "Sessions". Compare is `session_compare.rs`, confirm and run are
//! `session_run.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use cmdr_sync::{
    BulkAction, EntryKind, OverrideRefusal, Plan, PlanItem, RootRef, ScanOptions, ShowFilters, Snapshot, SyncControl,
    ViewRow, apply_overrides, bulk_changes, check, effective, is_visible, legal_actions, summarize, view_rows,
};

use super::wire::{CompareRequest, SyncError, SyncRowWire, SyncRowsPage, SyncState, ViewRowWire};

/// The open sessions, by id.
static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<Session>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Pairs of roots with a run going on (either order), across sessions.
static RUNNING_PAIRS: LazyLock<Mutex<HashSet<PairKey>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// A pair of roots, order-free: a swapped dialog is the same two folders.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PairKey([(String, String); 2]);

impl PairKey {
    fn of(left: &RootRef, right: &RootRef) -> Self {
        let mut pair = [
            (left.volume_id.clone(), left.path.clone()),
            (right.volume_id.clone(), right.path.clone()),
        ];
        pair.sort();
        Self(pair)
    }
}

/// Holds a pair's run lock until dropped (a panicking run releases it too).
pub(crate) struct RunLock(PairKey);

impl RunLock {
    /// The lock for this pair, or `RunInProgress` when a run holds it.
    pub(crate) fn take(left: &RootRef, right: &RootRef) -> Result<Self, SyncError> {
        let key = PairKey::of(left, right);
        if RUNNING_PAIRS.lock_ignore_poison().insert(key.clone()) {
            Ok(Self(key))
        } else {
            Err(SyncError::RunInProgress)
        }
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        RUNNING_PAIRS.lock_ignore_poison().remove(&self.0);
    }
}

/// One side as a compare resolved it.
#[derive(Clone)]
pub(crate) struct SideSetup {
    /// What the dialog named.
    pub(crate) root_ref: RootRef,
    /// The connected volume.
    pub(crate) volume: Arc<dyn Volume>,
    /// The root as a path on it.
    pub(crate) root: PathBuf,
}

impl SideSetup {
    /// Whether the root is a path in this Mac's own namespace (local disk or a
    /// mounted share), where a custom versions folder could lie inside it.
    pub(crate) fn local_namespace(&self) -> bool {
        self.volume.local_path().is_some()
    }
}

/// A finished compare and the person's changes since.
pub(crate) struct Compared {
    pub(crate) left: SideSetup,
    pub(crate) right: SideSetup,
    pub(crate) left_snap: Arc<Snapshot>,
    pub(crate) right_snap: Arc<Snapshot>,
    /// The plan, with every override applied.
    pub(crate) plan: Plan,
    /// What the compare was asked (options, Settings, overrides).
    pub(crate) request: CompareRequest,
    /// The scan's options without the top-level selection: a re-check before
    /// a batch counts a folder exactly as the scan did.
    pub(crate) scan: ScanOptions,
    /// A run has changed the folders since.
    pub(crate) stale: bool,
}

/// One dialog's state.
pub(crate) struct Session {
    pub(crate) id: String,
    /// The running compare's control.
    pub(crate) compare: Mutex<Option<Arc<SyncControl>>>,
    /// Bumped by every compare; only the newest one's result is kept.
    pub(crate) generation: AtomicU64,
    pub(crate) compared: Mutex<Option<Compared>>,
    /// The running run's control.
    pub(crate) run: Mutex<Option<Arc<SyncControl>>>,
}

impl Session {
    pub(crate) fn open() -> Arc<Self> {
        let session = Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            compare: Mutex::new(None),
            generation: AtomicU64::new(0),
            compared: Mutex::new(None),
            run: Mutex::new(None),
        });
        SESSIONS
            .lock_ignore_poison()
            .insert(session.id.clone(), Arc::clone(&session));
        session
    }

    /// The next compare's generation.
    pub(crate) fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    pub(crate) fn is_running(&self) -> bool {
        self.run.lock_ignore_poison().is_some()
    }

    fn is_comparing(&self) -> bool {
        self.compare.lock_ignore_poison().is_some()
    }

    /// Runs `f` on the finished compare; refuses while a compare runs.
    pub(crate) fn with_compared<T>(
        &self,
        f: impl FnOnce(&mut Compared) -> Result<T, SyncError>,
    ) -> Result<T, SyncError> {
        if self.is_comparing() {
            return Err(SyncError::CompareRunning);
        }
        let mut compared = self.compared.lock_ignore_poison();
        match compared.as_mut() {
            Some(compared) => f(compared),
            None => Err(SyncError::NotCompared),
        }
    }
}

/// The open session with this id.
pub(crate) fn session(id: &str) -> Result<Arc<Session>, SyncError> {
    SESSIONS
        .lock_ignore_poison()
        .get(id)
        .cloned()
        .ok_or(SyncError::NoSuchSession)
}

/// Stops whatever the session runs and forgets it. A run already under way
/// stops at its next step and still puts back what it set aside.
pub(crate) fn close(id: &str) {
    let Some(session) = SESSIONS.lock_ignore_poison().remove(id) else {
        return;
    };
    for control in [&session.compare, &session.run] {
        if let Some(control) = control.lock_ignore_poison().as_ref() {
            control.cancel();
        }
    }
}

/// Pause or resume the session's compare or run.
pub(crate) fn set_paused(id: &str, run: bool, paused: bool) -> Result<(), SyncError> {
    let session = session(id)?;
    let slot = if run { &session.run } else { &session.compare };
    if let Some(control) = slot.lock_ignore_poison().as_ref() {
        control.set_paused(paused);
    }
    Ok(())
}

/// Cancel the session's compare or run.
pub(crate) fn cancel(id: &str, run: bool) -> Result<(), SyncError> {
    let session = session(id)?;
    let slot = if run { &session.run } else { &session.compare };
    if let Some(control) = slot.lock_ignore_poison().as_ref() {
        control.cancel();
    }
    Ok(())
}

fn name_of(rel_path: &str) -> String {
    rel_path.rsplit('/').next().unwrap_or(rel_path).to_owned()
}

fn item_row(plan: &Plan, item: &PlanItem) -> ViewRowWire {
    let size = |side: Option<&cmdr_sync::Entry>| {
        side.and_then(|entry| match entry.facts.kind {
            EntryKind::Folder => entry.subtree.map(|subtree| subtree.bytes),
            EntryKind::File | EntryKind::Symlink => entry.facts.size,
        })
    };
    let (left, right) = (item.left.as_ref(), item.right.as_ref());
    ViewRowWire::Item {
        index: item.index,
        left_name: left.map(|entry| name_of(&entry.rel_path)),
        right_name: right.map(|entry| name_of(&entry.rel_path)),
        left_kind: left.map(|entry| entry.facts.kind),
        right_kind: right.map(|entry| entry.facts.kind),
        left_size: size(left),
        right_size: size(right),
        left_mtime: left.and_then(|entry| entry.facts.mtime),
        right_mtime: right.and_then(|entry| entry.facts.mtime),
        category: item.verdict.category,
        rung: item.verdict.rung,
        content: item.content,
        same_size: item.verdict.same_size,
        action: item.action,
        default_action: item.default_action,
        older_overwrites_newer: item.older_overwrites_newer,
        legal: legal_actions(plan, item),
    }
}

/// Rows `start..end` of the grid under `filters`.
pub(crate) fn rows(id: &str, start: u32, end: u32, filters: &ShowFilters) -> Result<SyncRowsPage, SyncError> {
    session(id)?.with_compared(|compared| {
        let plan = &compared.plan;
        let view = view_rows(plan, filters);
        let total = u32::try_from(view.len()).unwrap_or(u32::MAX);
        let (start, end) = (start.min(total), end.clamp(start.min(total), total));
        let rows = view[start as usize..end as usize]
            .iter()
            .zip(start..)
            .map(|(row, view_index)| SyncRowWire {
                view_index,
                row: match row {
                    ViewRow::Folder {
                        rel_dir, ordinal, of, ..
                    } => ViewRowWire::Folder {
                        rel_dir: rel_dir.clone(),
                        ordinal: *ordinal,
                        of: *of,
                    },
                    ViewRow::Item { index } => item_row(plan, &plan.items[*index as usize]),
                },
            })
            .collect();
        Ok(SyncRowsPage { total, rows })
    })
}

/// The summary and the guards of the current plan.
pub(crate) fn state_of(compared: &Compared, filters: &ShowFilters) -> SyncState {
    let request = &compared.request;
    let guards = effective(&request.defaults, &request.options, &request.overrides).guards;
    SyncState {
        summary: summarize(&compared.plan, filters),
        guards: check(&compared.plan, &compared.left_snap, &compared.right_snap, &guards),
        stale: compared.stale,
    }
}

pub(crate) fn state(id: &str, filters: &ShowFilters) -> Result<SyncState, SyncError> {
    session(id)?.with_compared(|compared| Ok(state_of(compared, filters)))
}

/// Applies `bulk` to the rows the person selected and can see.
pub(crate) fn set_actions(
    id: &str,
    items: &[u32],
    bulk: BulkAction,
    filters: &ShowFilters,
) -> Result<SyncState, SyncError> {
    let session = session(id)?;
    if session.is_running() {
        return Err(SyncError::RunInProgress);
    }
    session.with_compared(|compared| {
        let plan = &compared.plan;
        if let Some(&item) = items.iter().find(|&&item| plan.items.get(item as usize).is_none()) {
            let refusal = OverrideRefusal::NoSuchItem { item };
            return Err(SyncError::Override { refusal });
        }
        // The contract of `bulk_changes`: only rows the person can see.
        let seen: Vec<u32> = items
            .iter()
            .copied()
            .filter(|&item| is_visible(&plan.items[item as usize], filters))
            .collect();
        let changes = bulk_changes(plan, &seen, bulk);
        compared.plan = apply_overrides(plan, &changes).map_err(|refusal| SyncError::Override { refusal })?;
        Ok(state_of(compared, filters))
    })
}

/// Whether `path` is a usable root: not empty, never climbing with `..`.
pub(crate) fn root_path_ok(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| part != std::path::Component::ParentDir)
}
