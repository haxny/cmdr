//! Starting a compare: every input checked first (volumes, roots, masks,
//! Settings, the panes' selections), then both sides scanned at once, planned,
//! and byte-compared where asked. Progress goes out at most every 100 ms.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{Volume, root_anchored};
use cmdr_sync::{
    CompareBy, ContentProgress, MaskFilter, Mode, NameRule, PlanOptions, ProfileOverrides, RootConflict, RootRef,
    ScanError, ScanOptions, ScanProgress, Side, Snapshot, SyncCaps, SyncControl, SyncDefaults, compare_contents,
    content_jobs, content_workers, effective, plan, reclassify, roots_conflict, scan, validate_defaults,
};

use super::host::{SyncEvent, SyncHost};
use super::session::{Compared, Session, SideSetup, next_revision, root_path_ok, session};
use super::wire::{
    CompareOutcome, CompareRequest, CompareStarted, SideRequest, SyncCompareFinished, SyncCompareProgress, SyncError,
};

/// The shortest gap between two progress events.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// How long resolving a root's real path may take (a hung mount blocks it).
const REAL_PATH_WAIT: Duration = Duration::from_secs(5);

/// Where `root` really is in this Mac's namespace (symlinks and aliases
/// resolved), for a volume with a local path; `Ok(None)` for a remote one.
async fn real_root(volume: &Arc<dyn Volume>, root: &Path) -> Result<Option<PathBuf>, ()> {
    let Some(local) = volume.local_path() else {
        return Ok(None);
    };
    let os_path = root_anchored(&local, root);
    let resolved = tokio::time::timeout(
        REAL_PATH_WAIT,
        tokio::task::spawn_blocking(move || std::fs::canonicalize(os_path)),
    )
    .await;
    match resolved {
        Ok(Ok(Ok(real))) => Ok(Some(real)),
        _ => Err(()),
    }
}

/// Same folder or nested: by real path when both sides are in this Mac's
/// namespace (whatever volume ids named them), else on one volume by path.
fn roots_overlap(left: &SideSetup, right: &SideSetup) -> Option<RootConflict> {
    match (&left.real_root, &right.real_root) {
        (Some(left), Some(right)) => {
            let local = |path: &PathBuf| RootRef {
                volume_id: String::new(),
                path: path.to_string_lossy().into_owned(),
            };
            roots_conflict(&local(left), &local(right))
        }
        _ => roots_conflict(&left.root_ref, &right.root_ref),
    }
}

/// The Settings values with the profile's overrides on top, as one
/// `SyncDefaults`, so one check covers both.
pub(crate) fn merged_settings(defaults: &SyncDefaults, overrides: &ProfileOverrides) -> SyncDefaults {
    SyncDefaults {
        keep: overrides.keep.clone().unwrap_or_else(|| defaults.keep.clone()),
        time_tolerance_secs: overrides.time_tolerance_secs.unwrap_or(defaults.time_tolerance_secs),
        ignore_hour_shift: overrides.ignore_hour_shift.unwrap_or(defaults.ignore_hour_shift),
        guards: overrides.guards.clone().unwrap_or_else(|| defaults.guards.clone()),
    }
}

/// Out-of-range Settings or overrides are refused by name (`effective` would
/// only clamp them).
pub(crate) fn check_settings(defaults: &SyncDefaults, overrides: &ProfileOverrides) -> Result<(), SyncError> {
    validate_defaults(&merged_settings(defaults, overrides)).map_err(|problem| SyncError::BadSettings { problem })
}

/// Everything checked, ready to scan.
struct Prepared {
    left: SideSetup,
    right: SideSetup,
    left_options: ScanOptions,
    right_options: ScanOptions,
    scan: ScanOptions,
    plan_options: PlanOptions,
}

async fn resolve_side(host: &dyn SyncHost, side: Side, request: &SideRequest) -> Result<SideSetup, SyncError> {
    if !root_path_ok(&request.root.path) {
        return Err(SyncError::BadPath { side });
    }
    let volume_id = request.root.volume_id.clone();
    let volume = host
        .volume(&volume_id)
        .ok_or(SyncError::VolumeNotConnected { side, volume_id })?;
    let root = PathBuf::from(&request.root.path);
    let real_root = real_root(&volume, &root)
        .await
        .map_err(|()| SyncError::RootUnreachable { side })?;
    Ok(SideSetup {
        root_ref: request.root.clone(),
        volume,
        real_root,
        root,
    })
}

/// The names "Only selected" restricts a side to, each one plain.
fn selected(
    host: &dyn SyncHost,
    side: Side,
    request: &SideRequest,
    include_hidden: bool,
) -> Result<Option<Vec<String>>, SyncError> {
    let Some(selection) = &request.selection else {
        return Ok(None);
    };
    let names = host
        .selection_names(&request.root, selection, include_hidden)
        .ok_or(SyncError::StaleSelection { side })?;
    let plain = |name: &String| !name.is_empty() && name != "." && name != ".." && !name.contains('/');
    if !names.iter().all(plain) {
        return Err(SyncError::StaleSelection { side });
    }
    Ok(Some(names))
}

/// The side a mode must be able to write to.
fn check_writable(mode: Mode, left: &SyncCaps, right: &SyncCaps) -> Result<(), SyncError> {
    let refused = match mode {
        Mode::CompareOnly => false,
        Mode::Update | Mode::Mirror => !right.can_write,
        Mode::BothWays => !left.can_write && !right.can_write,
    };
    if refused {
        return Err(SyncError::SideReadOnly { side: Side::Right });
    }
    Ok(())
}

async fn prepare(host: &dyn SyncHost, request: &CompareRequest) -> Result<Prepared, SyncError> {
    let left = resolve_side(host, Side::Left, &request.left).await?;
    let right = resolve_side(host, Side::Right, &request.right).await?;
    if let Some(conflict) = roots_overlap(&left, &right) {
        return Err(SyncError::RootsConflict { conflict });
    }
    let options = &request.options;
    let filter = MaskFilter::parse(&options.masks, &options.excludes).map_err(|error| SyncError::BadMask { error })?;
    check_settings(&request.defaults, &request.overrides)?;
    let (left_caps, right_caps) = (SyncCaps::of(left.volume.as_ref()), SyncCaps::of(right.volume.as_ref()));
    check_writable(options.mode, &left_caps, &right_caps)?;
    let (left_only, right_only) = if options.only_selected {
        let hidden = request.include_hidden;
        (
            selected(host, Side::Left, &request.left, hidden)?,
            selected(host, Side::Right, &request.right, hidden)?,
        )
    } else {
        (None, None)
    };
    let effective = effective(&request.defaults, options, &request.overrides);
    let scan = ScanOptions {
        subfolders: options.subfolders,
        include_hidden: request.include_hidden,
        empty_folders: options.empty_folders,
        filter,
        name_rule: NameRule::for_pair(&left_caps, &right_caps),
        only: None,
    };
    Ok(Prepared {
        left_options: ScanOptions {
            only: left_only,
            ..scan.clone()
        },
        right_options: ScanOptions {
            only: right_only,
            ..scan.clone()
        },
        scan,
        plan_options: PlanOptions {
            mode: options.mode,
            rules: effective.rules.widened_for(&left_caps, &right_caps),
        },
        left,
        right,
    })
}

/// Validates the request and starts the compare in its session (a new one
/// unless `session_id` names one; a newer compare there stops the older, whose
/// events then never go out). `window` owns a new session.
pub(crate) async fn start(
    host: Arc<dyn SyncHost>,
    request: CompareRequest,
    window: Option<String>,
) -> Result<CompareStarted, SyncError> {
    let existing = request.session_id.as_deref().map(session).transpose()?;
    if existing.as_ref().is_some_and(|session| session.is_running()) {
        return Err(SyncError::RunInProgress);
    }
    // Checked before a new session opens, so a refused request leaves none behind.
    let prepared = prepare(host.as_ref(), &request).await?;
    let session = existing.unwrap_or_else(|| Session::open(window));
    let control = SyncControl::new();
    let generation = session.next_generation();
    if let Some(older) = session.compare.lock_ignore_poison().replace(Arc::clone(&control)) {
        older.cancel();
    }
    let id = session.id.clone();
    tokio::spawn(async move {
        let outcome = run(host.as_ref(), &session, generation, prepared, request, &control).await;
        let mut slot = session.compare.lock_ignore_poison();
        if slot.as_ref().is_some_and(|running| Arc::ptr_eq(running, &control)) {
            *slot = None;
        }
        drop(slot);
        // A superseded compare says nothing: the dialog hears only the newest.
        if session.is_current(generation) {
            host.emit(SyncEvent::CompareFinished(SyncCompareFinished {
                session_id: session.id.clone(),
                generation,
                outcome,
            }));
        }
    });
    Ok(CompareStarted {
        session_id: id,
        generation,
    })
}

/// The latest counts, sent at most every `PROGRESS_EVERY`.
struct Progress<'a> {
    host: &'a dyn SyncHost,
    session: &'a Session,
    generation: u64,
    state: Mutex<(SyncCompareProgress, Option<Instant>)>,
}

impl<'a> Progress<'a> {
    fn new(host: &'a dyn SyncHost, session: &'a Session, generation: u64) -> Self {
        let zero = |side| ScanProgress {
            side,
            folders: 0,
            files: 0,
            bytes: 0,
        };
        let event = SyncCompareProgress {
            session_id: session.id.clone(),
            generation,
            left: zero(Side::Left),
            right: zero(Side::Right),
            content: None,
        };
        Self {
            host,
            session,
            generation,
            state: Mutex::new((event, None)),
        }
    }

    fn update(&self, change: impl FnOnce(&mut SyncCompareProgress)) {
        let mut state = self.state.lock_ignore_poison();
        change(&mut state.0);
        let due =
            state.1.is_none_or(|last| last.elapsed() >= PROGRESS_EVERY) && self.session.is_current(self.generation);
        if due {
            state.1 = Some(Instant::now());
            self.host.emit(SyncEvent::CompareProgress(state.0.clone()));
        }
    }

    fn scan(&self, progress: ScanProgress) {
        self.update(|event| match progress.side {
            Side::Left => event.left = progress,
            Side::Right => event.right = progress,
        });
    }

    fn content(&self, progress: ContentProgress) {
        self.update(|event| event.content = Some(progress));
    }
}

fn scan_outcome(error: ScanError) -> CompareOutcome {
    match error {
        ScanError::Stopped => CompareOutcome::Stopped,
        ScanError::RootUnreachable { side, .. } => CompareOutcome::Refused {
            error: SyncError::RootUnreachable { side },
        },
    }
}

async fn run(
    host: &dyn SyncHost,
    session: &Session,
    generation: u64,
    prepared: Prepared,
    request: CompareRequest,
    control: &Arc<SyncControl>,
) -> CompareOutcome {
    let progress = Progress::new(host, session, generation);
    let on_scan = |update: ScanProgress| progress.scan(update);
    let (left, right) = (&prepared.left, &prepared.right);
    let (left_snap, right_snap) = tokio::join!(
        scan(
            Side::Left,
            left.volume.as_ref(),
            &left.root,
            &prepared.left_options,
            control,
            &on_scan
        ),
        scan(
            Side::Right,
            right.volume.as_ref(),
            &right.root,
            &prepared.right_options,
            control,
            &on_scan
        ),
    );
    let (left_snap, right_snap) = match (left_snap, right_snap) {
        (Ok(left), Ok(right)) => (left, right),
        (Err(error), _) | (_, Err(error)) => return scan_outcome(error),
    };
    let mut planned = plan(&left_snap, &right_snap, None, &prepared.plan_options);
    if prepared.plan_options.rules.by == CompareBy::Content {
        planned = compare_content(&prepared, &left_snap, &right_snap, planned, control, &progress).await;
    }
    if control.is_cancelled() || !session.is_current(generation) {
        return CompareOutcome::Stopped;
    }
    *session.compared.lock_ignore_poison() = Some(Compared {
        left: prepared.left.clone(),
        right: prepared.right.clone(),
        left_snap: Arc::new(left_snap),
        right_snap: Arc::new(right_snap),
        plan: planned,
        revision: next_revision(),
        confirmed: None,
        view: None,
        request,
        scan: prepared.scan,
        stale: false,
    });
    CompareOutcome::Done
}

async fn compare_content(
    prepared: &Prepared,
    left_snap: &Snapshot,
    right_snap: &Snapshot,
    planned: cmdr_sync::Plan,
    control: &Arc<SyncControl>,
    progress: &Progress<'_>,
) -> cmdr_sync::Plan {
    let (left, right) = (&prepared.left, &prepared.right);
    let jobs = content_jobs(&planned, &left_snap.root, &right_snap.root);
    if jobs.is_empty() {
        return planned;
    }
    let workers = content_workers(left.volume.as_ref(), right.volume.as_ref());
    let on_content = |update: ContentProgress| progress.content(update);
    let answers = compare_contents(
        left.volume.as_ref(),
        right.volume.as_ref(),
        jobs,
        workers,
        control,
        &on_content,
    )
    .await;
    reclassify(&planned, &answers).plan
}
