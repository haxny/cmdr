//! The confirm and the run. Both read the CURRENT plan (every override
//! applied); the run calls `gate` itself at accept time, so no report made
//! earlier can wave a removal through. `DETAILS.md` § "Sessions".

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Datelike, Timelike};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_sync::{
    Action, ContentJob, ContentState, EntryKind, KeepDestination, Place, PlanItem, RunContext, RunItemResult,
    RunReport, RunSide, RunStamp, ShowFilters, Side, SyncControl, SyncDefaults, check, compare_contents, effective,
    execute, gate, keep_destination, reclassify, summarize,
};

use super::engine::{EngineAdapter, EngineConfig, EngineSide};
use super::host::{LOCAL_DISK_VOLUME_ID, SyncEvent, SyncHost};
use super::real_paths::PathCheck;
use super::run_lock::{RootKey, RunLock};
use super::session::{Compared, Session, SideSetup, session};
use super::session_compare::check_settings;
use super::wire::{
    ConfirmGroup, ConfirmGroupKey, ConfirmPreview, KeepDestinationWire, RunItemWire, RunReportWire, RunRequest,
    SyncError, SyncRowsChanged, SyncRunFinished, SyncRunProgress, WarnedItem,
};

/// Rows of "Older overwrites newer" the confirm lists by name.
const WARNED_LISTED: usize = 200;

/// The confirm's group order.
const GROUP_ORDER: [ConfirmGroupKey; 8] = [
    ConfirmGroupKey::CopyRight,
    ConfirmGroupKey::ReplaceRight,
    ConfirmGroupKey::RemoveRight,
    ConfirmGroupKey::CopyRightRecreates,
    ConfirmGroupKey::CopyLeft,
    ConfirmGroupKey::ReplaceLeft,
    ConfirmGroupKey::RemoveLeft,
    ConfirmGroupKey::CopyLeftRecreates,
];

/// The confirm group a row's action falls in; `None` when it does nothing. A
/// copy that turns the mode's removal around ("recreates") is its own group.
pub(crate) fn group_of(item: &PlanItem) -> Option<ConfirmGroupKey> {
    let key = match item.action {
        Action::CopyRight if item.default_action == Action::RemoveLeft => ConfirmGroupKey::CopyRightRecreates,
        Action::CopyLeft if item.default_action == Action::RemoveRight => ConfirmGroupKey::CopyLeftRecreates,
        Action::CopyRight if item.right.is_some() => ConfirmGroupKey::ReplaceRight,
        Action::CopyLeft if item.left.is_some() => ConfirmGroupKey::ReplaceLeft,
        Action::CopyRight => ConfirmGroupKey::CopyRight,
        Action::CopyLeft => ConfirmGroupKey::CopyLeft,
        Action::RemoveRight => ConfirmGroupKey::RemoveRight,
        Action::RemoveLeft => ConfirmGroupKey::RemoveLeft,
        Action::Equal | Action::Skip | Action::Undecided => return None,
    };
    Some(key)
}

fn run_stamp() -> RunStamp {
    let now = chrono::Local::now();
    let part = |value: u32| u8::try_from(value).unwrap_or(0);
    RunStamp::new(
        now.year(),
        part(now.month()),
        part(now.day()),
        part(now.hour()),
        part(now.minute()),
        part(now.second()),
    )
}

fn run_side(setup: &SideSetup, snap_caps: cmdr_sync::SyncCaps) -> RunSide {
    RunSide {
        root: setup.root.clone(),
        caps: snap_caps,
        namespace_root: setup.real_root.clone(),
    }
}

fn context(compared: &Compared, defaults: &SyncDefaults) -> Result<RunContext, SyncError> {
    let request = &compared.request;
    let keep = effective(defaults, &request.options, &request.overrides).keep;
    let left = run_side(&compared.left, compared.plan.left_caps);
    let right = run_side(&compared.right, compared.plan.right_caps);
    let ctx = RunContext::new(left, right, keep, run_stamp(), request.profile_name.clone())
        .map_err(|refusal| SyncError::Keep { refusal })?;
    Ok(ctx)
}

fn setup_of(compared: &Compared, side: Side) -> &SideSetup {
    match side {
        Side::Left => &compared.left,
        Side::Right => &compared.right,
    }
}

fn volume_id_at(compared: &Compared, side: Side, place: Place) -> String {
    match place {
        Place::Target => setup_of(compared, side).root_ref.volume_id.clone(),
        Place::LocalDisk => LOCAL_DISK_VOLUME_ID.to_owned(),
    }
}

/// Where a group's replaced or removed items go (the dated folder, not each
/// file's spot in it).
fn keep_of(compared: &Compared, ctx: &RunContext, item: &PlanItem, side: Side) -> Option<KeepDestinationWire> {
    let entry = item.entry(side)?;
    let keep = &ctx.keep;
    match keep_destination(keep, ctx.caps(side), ctx.versions(side), &entry.rel_path) {
        KeepDestination::Trash => Some(KeepDestinationWire::Trash),
        KeepDestination::Folder { place, .. } => Some(KeepDestinationWire::Folder {
            volume_id: volume_id_at(compared, side, place),
            path: ctx.versions(side).dir.to_string_lossy().into_owned(),
        }),
        KeepDestination::Refused(_) => None,
    }
}

fn add_to_group(
    groups: &mut Vec<ConfirmGroup>,
    key: ConfirmGroupKey,
    item: &PlanItem,
    keep: Option<KeepDestinationWire>,
) {
    let files = item.action.needs().map_or(0, |side| item.files_on(side));
    match groups.iter_mut().find(|group| group.key == key) {
        Some(group) => {
            group.files += files;
            group.bytes += item.bytes;
        }
        None => groups.push(ConfirmGroup {
            key,
            files,
            bytes: item.bytes,
            keep,
        }),
    }
}

fn keeps_something(key: ConfirmGroupKey) -> bool {
    matches!(
        key,
        ConfirmGroupKey::ReplaceRight
            | ConfirmGroupKey::ReplaceLeft
            | ConfirmGroupKey::RemoveRight
            | ConfirmGroupKey::RemoveLeft
    )
}

/// What the confirm shows for the current plan. A custom versions folder is
/// resolved off the session lock first.
pub(crate) async fn confirm(
    id: &str,
    defaults: &SyncDefaults,
    filters: &ShowFilters,
) -> Result<ConfirmPreview, SyncError> {
    let session = session(id)?;
    let paths = session.with_compared(|compared| {
        check_settings(defaults, &compared.request.overrides)?;
        Ok(PathCheck::of(compared, &context(compared, defaults)?, false))
    })?;
    paths.verify().await?;
    session.with_compared(|compared| {
        check_settings(defaults, &compared.request.overrides)?;
        let ctx = context(compared, defaults)?;
        // The run must be shown with exactly these Settings: other ones are a
        // new confirm.
        if compared.confirmed.as_ref() != Some(defaults) {
            compared.confirmed = Some(defaults.clone());
            compared.bump();
        }
        let request = &compared.request;
        let guards = effective(defaults, &request.options, &request.overrides).guards;
        let report = check(&compared.plan, &compared.left_snap, &compared.right_snap, &guards);
        let mut groups = Vec::new();
        let mut warned = Vec::new();
        let mut warned_total = 0;
        for item in &compared.plan.items {
            let (Some(key), Some(target)) = (group_of(item), item.action.target()) else {
                continue;
            };
            let keep = keeps_something(key)
                .then(|| keep_of(compared, &ctx, item, target))
                .flatten();
            add_to_group(&mut groups, key, item, keep);
            if item.older_overwrites_newer {
                warned_total += 1;
                if warned.len() < WARNED_LISTED {
                    warned.push(warned_item(item));
                }
            }
        }
        groups.sort_by_key(|group| GROUP_ORDER.iter().position(|key| *key == group.key));
        let keep_is_slow = groups.iter().any(|group| {
            matches!(&group.keep, Some(KeepDestinationWire::Folder { volume_id, .. })
                if group_side(group.key).is_some_and(|side| *volume_id != setup_of(compared, side).root_ref.volume_id))
        });
        Ok(ConfirmPreview {
            revision: compared.revision,
            groups,
            older_overwrites_newer: warned,
            older_overwrites_newer_total: warned_total,
            blocks: report.blocks,
            skew: report.skew,
            hidden_actions: summarize(&compared.plan, filters).hidden_actions,
            keep_is_slow,
        })
    })
}

fn group_side(key: ConfirmGroupKey) -> Option<Side> {
    match key {
        ConfirmGroupKey::ReplaceRight | ConfirmGroupKey::RemoveRight => Some(Side::Right),
        ConfirmGroupKey::ReplaceLeft | ConfirmGroupKey::RemoveLeft => Some(Side::Left),
        _ => None,
    }
}

fn warned_item(item: &PlanItem) -> WarnedItem {
    let entry = item.left.as_ref().or(item.right.as_ref());
    WarnedItem {
        item: item.index,
        rel_path: entry.map(|entry| entry.rel_path.clone()).unwrap_or_default(),
        left_mtime: item.left.as_ref().and_then(|entry| entry.facts.mtime),
        right_mtime: item.right.as_ref().and_then(|entry| entry.facts.mtime),
    }
}

/// Everything a run task takes with it.
struct Launch {
    plan: cmdr_sync::Plan,
    run: Vec<u32>,
    held: u64,
    ctx: RunContext,
    adapter: EngineAdapter,
    control: Arc<SyncControl>,
    lock: RunLock,
}

fn engine_side(setup: &SideSetup) -> EngineSide {
    EngineSide {
        volume_id: setup.root_ref.volume_id.clone(),
        volume: Arc::clone(&setup.volume),
        root: setup.root.clone(),
    }
}

/// The volume the compare read must still be the one under its id: a volume
/// that went away and came back (or another one under the id) needs a new
/// compare.
fn still_connected(host: &dyn SyncHost, side: Side, setup: &SideSetup) -> Result<(), SyncError> {
    let volume_id = setup.root_ref.volume_id.clone();
    match host.volume(&volume_id) {
        Some(now) if Arc::ptr_eq(&now, &setup.volume) => Ok(()),
        _ => Err(SyncError::VolumeNotConnected { side, volume_id }),
    }
}

/// Whether `request` still names the plan the session holds: not stale, the
/// revision and Settings of the confirm the person saw.
fn still_confirmed(compared: &Compared, request: &RunRequest) -> Result<(), SyncError> {
    if compared.stale {
        return Err(SyncError::StaleCompare);
    }
    if request.revision != compared.revision || compared.confirmed.as_ref() != Some(&request.defaults) {
        return Err(SyncError::PlanChanged);
    }
    check_settings(&request.defaults, &compared.request.overrides)
}

/// Gates the current plan, keeps the accepted groups, and builds the engine.
/// `ctx` and the real paths were checked off the lock, for this same revision.
fn launch(
    host: &dyn SyncHost,
    compared: &Compared,
    request: &RunRequest,
    ctx: RunContext,
) -> Result<Launch, SyncError> {
    still_confirmed(compared, request)?;
    let operations = host.operations().ok_or(SyncError::NotInstalled)?;
    let local = host.volume(LOCAL_DISK_VOLUME_ID).ok_or(SyncError::NotInstalled)?;
    for side in [Side::Left, Side::Right] {
        still_connected(host, side, setup_of(compared, side))?;
    }
    let lock = RunLock::take([RootKey::of(&compared.left), RootKey::of(&compared.right)])?;
    let settings = &compared.request;
    let guards = effective(&request.defaults, &settings.options, &settings.overrides).guards;
    let plan = compared.plan.clone();
    let gated = gate(
        &plan,
        &compared.left_snap,
        &compared.right_snap,
        &guards,
        &request.acknowledged,
    );
    let accepted = |index: u32| group_of(&plan.items[index as usize]).is_some_and(|key| request.groups.contains(&key));
    let run: Vec<u32> = gated.run.iter().copied().filter(|&index| accepted(index)).collect();
    let held = gated.held.iter().filter(|(index, _)| accepted(*index)).count() as u64;
    let control = SyncControl::new();
    let adapter = EngineAdapter::new(EngineConfig {
        operations,
        left: engine_side(&compared.left),
        right: engine_side(&compared.right),
        local: EngineSide {
            volume_id: LOCAL_DISK_VOLUME_ID.to_owned(),
            volume: local,
            root: PathBuf::from("/"),
        },
        scan: compared.scan.clone(),
        control: Arc::clone(&control),
        sinks: host.sinks(),
    });
    Ok(Launch {
        plan,
        run,
        held,
        ctx,
        adapter,
        control,
        lock,
    })
}

/// Starts the run of the accepted groups on a task of its own. Both roots are
/// resolved again first, off the session lock: a root swapped since the compare
/// (for a symlink to elsewhere, say) is a stale compare.
pub(crate) async fn start_run(host: Arc<dyn SyncHost>, id: &str, request: &RunRequest) -> Result<(), SyncError> {
    let session = session(id)?;
    if session.is_running() {
        return Err(SyncError::RunInProgress);
    }
    let (ctx, paths) = session.with_compared(|compared| {
        still_confirmed(compared, request)?;
        let ctx = context(compared, &request.defaults)?;
        let paths = PathCheck::of(compared, &ctx, true);
        Ok((ctx, paths))
    })?;
    paths.verify().await?;
    let launched = {
        let mut running = session.run.lock_ignore_poison();
        if running.is_some() {
            return Err(SyncError::RunInProgress);
        }
        // `launch` checks the revision again, so a change during the checks above
        // is `PlanChanged`, and a compare already running is `CompareRunning`.
        // A compare can still slip in while this run starts (its own checks
        // passed before `run` was set): that is safe. The run executes its own
        // copy of the plan, and at its end marks whatever compare the session
        // then holds as stale, so no later run can use a plan scanned while
        // this one was writing.
        let launched = session.with_compared(|compared| launch(host.as_ref(), compared, request, ctx))?;
        *running = Some(Arc::clone(&launched.control));
        launched
    };
    tokio::spawn(async move {
        let Launch {
            plan,
            run,
            held,
            ctx,
            adapter,
            control,
            lock,
        } = launched;
        let session_id = session.id.clone();
        let on_progress = |progress| {
            host.emit(SyncEvent::RunProgress(SyncRunProgress {
                session_id: session_id.clone(),
                progress,
            }));
        };
        let report = execute(&plan, &run, &ctx, &adapter, &control, &on_progress).await;
        let wire = report_wire(&report, held, &adapter);
        drop(lock);
        if let Some(compared) = session.compared.lock_ignore_poison().as_mut() {
            compared.stale = true;
        }
        *session.run.lock_ignore_poison() = None;
        host.emit(SyncEvent::RunFinished(SyncRunFinished {
            session_id,
            report: wire,
        }));
    });
    Ok(())
}

fn versions_wire(dir: &Path, place: Place, sides: [(&Path, String); 2]) -> KeepDestinationWire {
    let volume_id = match place {
        Place::LocalDisk => LOCAL_DISK_VOLUME_ID.to_owned(),
        Place::Target => sides
            .iter()
            .find(|(root, _)| dir.starts_with(root))
            .map(|(_, id)| id.clone())
            .unwrap_or_default(),
    };
    KeepDestinationWire::Folder {
        volume_id,
        path: dir.to_string_lossy().into_owned(),
    }
}

fn report_wire(report: &RunReport, held: u64, adapter: &EngineAdapter) -> RunReportWire {
    let mut wire = RunReportWire {
        held,
        kept_bytes: report.kept_bytes,
        operation_ids: adapter.operation_ids(),
        stopped: report.stopped,
        ..RunReportWire::default()
    };
    for &(item, result) in &report.results {
        wire.items.push(RunItemWire { item, result });
        let count = match result {
            RunItemResult::Done => &mut wire.done,
            RunItemResult::ChangedSinceCompare => &mut wire.changed_since_compare,
            RunItemResult::NoTrash => &mut wire.no_trash,
            RunItemResult::CouldNotDo | RunItemResult::VersionsSpotTaken | RunItemResult::VersionsFolderUnsafe => {
                &mut wire.could_not_do
            }
            RunItemResult::KeptNotReplaced => &mut wire.kept_not_replaced,
            RunItemResult::HeldAfterFailedCopy => &mut wire.held,
            RunItemResult::NotStarted => &mut wire.not_started,
        };
        *count += 1;
    }
    let config = &adapter.config;
    let sides = [
        (config.left.root.as_path(), config.left.volume_id.clone()),
        (config.right.root.as_path(), config.right.volume_id.clone()),
    ];
    wire.versions_folders = report
        .versions_used
        .iter()
        .map(|root| versions_wire(&root.dir, root.place, sides.clone()))
        .collect();
    wire
}

/// What a row's content compare was started on, to check the answer still
/// belongs to that row.
pub(crate) struct ContentTicket {
    generation: u64,
    item: u32,
    paths: (String, String),
}

/// Compares one equal-size file pair byte by byte and shows the answer on its row.
pub(crate) async fn compare_item_content(host: Arc<dyn SyncHost>, id: &str, item: u32) -> Result<(), SyncError> {
    let session = session(id)?;
    if session.is_running() {
        return Err(SyncError::RunInProgress);
    }
    let generation = session.generation();
    let (job, ticket, left, right) = session.with_compared(|compared| {
        let row = compared
            .plan
            .items
            .get(item as usize)
            .ok_or(SyncError::NotContentComparable { item })?;
        let job = content_job(compared, row).ok_or(SyncError::NotContentComparable { item })?;
        let ticket = ContentTicket {
            generation,
            item,
            paths: rel_paths(row),
        };
        Ok((
            job,
            ticket,
            Arc::clone(&compared.left.volume),
            Arc::clone(&compared.right.volume),
        ))
    })?;
    let control = SyncControl::new();
    let answers = compare_contents(left.as_ref(), right.as_ref(), vec![job], 1, &control, &|_| {}).await;
    let state = answers.first().map_or(ContentState::Pending, |(_, state)| *state);
    apply_content(&session, &ticket, state)?;
    host.emit(SyncEvent::RowsChanged(SyncRowsChanged {
        session_id: session.id.clone(),
    }));
    Ok(())
}

fn rel_paths(row: &PlanItem) -> (String, String) {
    let rel = |entry: Option<&cmdr_sync::Entry>| entry.map(|entry| entry.rel_path.clone()).unwrap_or_default();
    (rel(row.left.as_ref()), rel(row.right.as_ref()))
}

/// Folds a content answer into the row it was asked for, unless a run, a newer
/// compare, or another row under that index came in between.
pub(crate) fn apply_content(session: &Session, ticket: &ContentTicket, state: ContentState) -> Result<(), SyncError> {
    if session.is_running() {
        return Err(SyncError::RunInProgress);
    }
    if !session.is_current(ticket.generation) {
        return Err(SyncError::PlanChanged);
    }
    session.with_compared(|compared| {
        let mut plan = compared.plan.clone();
        let row = plan.items.get_mut(ticket.item as usize).ok_or(SyncError::PlanChanged)?;
        if rel_paths(row) != ticket.paths {
            return Err(SyncError::PlanChanged);
        }
        row.content = ContentState::Pending;
        compared.plan = reclassify(&plan, &[(ticket.item, state)]).plan;
        compared.bump();
        Ok(())
    })
}

#[cfg(test)]
pub(crate) fn ticket_for(session: &Session, item: u32, paths: (&str, &str)) -> ContentTicket {
    ContentTicket {
        generation: session.generation(),
        item,
        paths: (paths.0.to_owned(), paths.1.to_owned()),
    }
}

fn content_job(compared: &Compared, row: &PlanItem) -> Option<ContentJob> {
    let (left, right) = (row.left.as_ref()?, row.right.as_ref()?);
    let files = left.facts.kind == EntryKind::File && right.facts.kind == EntryKind::File;
    let size = left.facts.size.filter(|size| right.facts.size == Some(*size))?;
    let readable = compared.plan.left_caps.can_read_content && compared.plan.right_caps.can_read_content;
    (files && readable).then(|| ContentJob {
        item: row.index,
        left: compared.left.root.join(&left.rel_path),
        right: compared.right.root.join(&right.rel_path),
        size,
    })
}
