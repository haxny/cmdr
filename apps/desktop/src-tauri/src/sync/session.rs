//! One Synchronize folders dialog = one session: its compare (snapshots and the
//! plan with the person's changes), the grid's pages, and at most one run.
//! Lifecycle, the run lock, and why the plan lives here: `DETAILS.md` §
//! "Sessions". Compare is `session_compare.rs`, confirm and run are
//! `session_run.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use cmdr_sync::{
    BulkAction, EntryKind, OverrideRefusal, Plan, PlanItem, RootRef, RowQuery, ScanOptions, ShowFilters, Snapshot,
    SyncControl, SyncDefaults, ViewRow, apply_overrides, bulk_outcome, check, effective, find_row, is_visible,
    legal_actions, summarize, view_rows,
};

use super::wire::{CompareRequest, SyncActionsOutcome, SyncError, SyncRowWire, SyncRowsPage, SyncState, ViewRowWire};

/// The open sessions, by id.
static SESSIONS: LazyLock<Mutex<HashMap<String, Arc<Session>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The most rows one `get_sync_rows` answers; the dialog pages 200 at a time.
pub(crate) const MAX_ROWS_PAGE: u32 = 2000;

/// Plan revisions, unique across sessions and compares.
static REVISIONS: AtomicU64 = AtomicU64::new(0);

/// A revision no plan has had yet.
pub(crate) fn next_revision() -> u64 {
    REVISIONS.fetch_add(1, Ordering::Relaxed) + 1
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
    /// Where the root really is in this Mac's namespace (symlinks resolved),
    /// for a volume with a local path; `None` for a remote one.
    pub(crate) real_root: Option<PathBuf>,
}

/// The grid rows of one plan revision under one set of show-filters.
pub(crate) struct ViewCache {
    revision: u64,
    filters: ShowFilters,
    rows: Arc<Vec<ViewRow>>,
}

/// A finished compare and the person's changes since.
pub(crate) struct Compared {
    pub(crate) left: SideSetup,
    pub(crate) right: SideSetup,
    pub(crate) left_snap: Arc<Snapshot>,
    pub(crate) right_snap: Arc<Snapshot>,
    /// The plan, with every override applied.
    pub(crate) plan: Plan,
    /// Bumped by every change to the plan or to the Settings the confirm read,
    /// so a run is tied to the confirm the person saw.
    pub(crate) revision: u64,
    /// The Settings the last confirm was shown with.
    pub(crate) confirmed: Option<SyncDefaults>,
    /// What the compare was asked (options, Settings, overrides).
    pub(crate) request: CompareRequest,
    /// The generation of the compare that made this plan: a key pressed on
    /// another compare's rows names another one and is refused.
    pub(crate) generation: u64,
    /// The scan's options without the top-level selection: a re-check before
    /// a batch counts a folder exactly as the scan did.
    pub(crate) scan: ScanOptions,
    /// A run has changed the folders since.
    pub(crate) stale: bool,
    pub(crate) view: Option<ViewCache>,
}

impl Compared {
    /// The plan changed.
    pub(crate) fn bump(&mut self) {
        self.revision = next_revision();
    }

    fn view(&mut self, filters: &ShowFilters) -> Arc<Vec<ViewRow>> {
        if let Some(cache) = &self.view
            && cache.revision == self.revision
            && cache.filters == *filters
        {
            return Arc::clone(&cache.rows);
        }
        let rows = Arc::new(view_rows(&self.plan, filters));
        self.view = Some(ViewCache {
            revision: self.revision,
            filters: *filters,
            rows: Arc::clone(&rows),
        });
        rows
    }
}

/// One dialog's state.
pub(crate) struct Session {
    pub(crate) id: String,
    /// The window whose dialog owns it; its end closes the session.
    window: Option<String>,
    /// The running compare's control.
    pub(crate) compare: Mutex<Option<Arc<SyncControl>>>,
    /// Bumped by every compare; only the newest one's result is kept.
    pub(crate) generation: AtomicU64,
    pub(crate) compared: Mutex<Option<Compared>>,
    /// The running run's control.
    pub(crate) run: Mutex<Option<Arc<SyncControl>>>,
    /// Single-row content compares under way (F-key "compare by content").
    pub(crate) row_compares: Mutex<Vec<Arc<SyncControl>>>,
    /// Set by `close` before it cancels anything: a compare or run that checks
    /// it under its slot's lock never starts on a closed session.
    closed: AtomicBool,
}

impl Session {
    pub(crate) fn open(window: Option<String>) -> Arc<Self> {
        let session = Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            window,
            compare: Mutex::new(None),
            generation: AtomicU64::new(0),
            compared: Mutex::new(None),
            run: Mutex::new(None),
            row_compares: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
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

    /// The newest compare's generation.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    /// `close` ran: nothing new may start here.
    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Refuses a window other than the one whose dialog opened the session.
    pub(crate) fn owned_by(&self, window: &str) -> Result<(), SyncError> {
        match &self.window {
            Some(owner) if owner != window => Err(SyncError::NoSuchSession),
            _ => Ok(()),
        }
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

/// The open session with this id, if `window` owns it: commands that change a
/// session come only from its own dialog (another window gets `NoSuchSession`).
pub(crate) fn owned_session(id: &str, window: &str) -> Result<Arc<Session>, SyncError> {
    let session = session(id)?;
    session.owned_by(window)?;
    Ok(session)
}

/// Stops whatever the session runs and forgets it. A run already under way
/// stops at its next step and still puts back what it set aside.
pub(crate) fn close(id: &str) {
    let Some(session) = SESSIONS.lock_ignore_poison().remove(id) else {
        return;
    };
    // First, so a compare or run starting now sees it under its slot's lock
    // (and refuses), or stored its control before we look (and is cancelled).
    session.closed.store(true, Ordering::Release);
    for control in [&session.compare, &session.run] {
        if let Some(control) = control.lock_ignore_poison().as_ref() {
            control.cancel();
        }
    }
    for control in session.row_compares.lock_ignore_poison().iter() {
        control.cancel();
    }
}

/// Closes every session of a window that went away.
pub(crate) fn close_for_window(label: &str) {
    let ids: Vec<String> = SESSIONS
        .lock_ignore_poison()
        .values()
        .filter(|session| session.window.as_deref() == Some(label))
        .map(|session| session.id.clone())
        .collect();
    for id in ids {
        close(&id);
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

/// Cancel the session's compare (with any single-row content compare) or run.
pub(crate) fn cancel(id: &str, run: bool) -> Result<(), SyncError> {
    let session = session(id)?;
    let slot = if run { &session.run } else { &session.compare };
    if let Some(control) = slot.lock_ignore_poison().as_ref() {
        control.cancel();
    }
    if !run {
        for control in session.row_compares.lock_ignore_poison().iter() {
            control.cancel();
        }
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
        left_rel_path: left.map(|entry| entry.rel_path.clone()),
        right_rel_path: right.map(|entry| entry.rel_path.clone()),
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

/// Rows `start..end` of the grid under `filters`, at most `MAX_ROWS_PAGE` of them.
pub(crate) fn rows(id: &str, start: u32, end: u32, filters: &ShowFilters) -> Result<SyncRowsPage, SyncError> {
    session(id)?.with_compared(|compared| {
        let view = compared.view(filters);
        let plan = &compared.plan;
        let total = u32::try_from(view.len()).unwrap_or(u32::MAX);
        let start = start.min(total);
        let end = end.min(start.saturating_add(MAX_ROWS_PAGE)).clamp(start, total);
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
        revision: compared.revision,
    }
}

pub(crate) fn state(id: &str, filters: &ShowFilters) -> Result<SyncState, SyncError> {
    session(id)?.with_compared(|compared| Ok(state_of(compared, filters)))
}

/// Applies `bulk` to the rows the person selected and can see, on the plan of
/// compare `generation` only: rows picked on an older compare's grid are
/// `PlanChanged`, never applied by index to the new plan.
pub(crate) fn set_actions(
    id: &str,
    generation: u64,
    items: &[u32],
    bulk: BulkAction,
    filters: &ShowFilters,
) -> Result<SyncActionsOutcome, SyncError> {
    let session = session(id)?;
    if session.is_running() {
        return Err(SyncError::RunInProgress);
    }
    session.with_compared(|compared| {
        if compared.generation != generation {
            return Err(SyncError::PlanChanged);
        }
        let plan = &compared.plan;
        if let Some(&item) = items.iter().find(|&&item| plan.items.get(item as usize).is_none()) {
            let refusal = OverrideRefusal::NoSuchItem { item };
            return Err(SyncError::Override { refusal });
        }
        // The contract of `bulk_outcome`: only rows the person can see.
        let seen: Vec<u32> = items
            .iter()
            .copied()
            .filter(|&item| is_visible(&plan.items[item as usize], filters))
            .collect();
        let outcome = bulk_outcome(plan, &seen, bulk);
        if !outcome.changes.is_empty() {
            compared.plan =
                apply_overrides(plan, &outcome.changes).map_err(|refusal| SyncError::Override { refusal })?;
            compared.bump();
        }
        Ok(SyncActionsOutcome {
            state: state_of(compared, filters),
            skipped: outcome.skipped,
            hidden: u32::try_from(items.len() - seen.len()).unwrap_or(u32::MAX),
        })
    })
}

/// The view row `query` lands on under `filters` (next or previous folder, a name), if any.
pub(crate) fn find(id: &str, filters: &ShowFilters, query: &RowQuery) -> Result<Option<u32>, SyncError> {
    session(id)?.with_compared(|compared| {
        let view = compared.view(filters);
        Ok(find_row(&compared.plan, &view, query))
    })
}

/// Whether `path` is a usable root: absolute (every volume path is, from the
/// volume's root), never climbing with `..`.
pub(crate) fn root_path_ok(path: &str) -> bool {
    let path = Path::new(path);
    path.is_absolute() && path.components().all(|part| part != std::path::Component::ParentDir)
}
