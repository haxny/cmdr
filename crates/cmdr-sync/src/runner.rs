//! The state machine behind `execute`: one step at a time, every item
//! re-checked against the snapshot right before its batch. Claims, keeps, and
//! restores live in `runner/keeping.rs`.

mod holds;
mod keeping;
mod tidy;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::steps::{landing, other};
use crate::{
    BatchOutcome, BoundSource, EngineBatch, Entry, EntryKind, ItemOutcome, KeepDestination, KeepRefusal, Observed,
    Place, Plan, PlanItem, RunContext, RunItemResult, RunProgress, RunReport, Side, Step, StepPlan, SyncControl,
    SyncEngine, VersionsRoot, steps,
};

pub(crate) struct Runner<'a> {
    plan: &'a Plan,
    ctx: RunContext,
    engine: &'a dyn SyncEngine,
    control: &'a SyncControl,
    on_progress: &'a (dyn Fn(RunProgress) + Sync),
    acting: BTreeSet<u32>,
    results: BTreeMap<u32, RunItemResult>,
    /// Replaced files waiting in their staging folder for the copy.
    kept: BTreeMap<u32, PathBuf>,
    /// Folders that couldn't be claimed, made, or trusted, and what that means
    /// for everything beneath them.
    blocked: Vec<(Place, Side, PathBuf, RunItemResult)>,
    /// Copies that didn't land, which hold related removals (`holds.rs`).
    holds: Option<Vec<holds::Hold>>,
    /// Target folders this run created for copies, and where copies landed.
    made_for_copies: BTreeSet<(Side, PathBuf)>,
    /// Dated folders this run claimed, and the folders it made inside them.
    claimed: Vec<(Side, VersionsRoot)>,
    keep_dirs: Vec<(Place, Side, PathBuf)>,
    leftover_folders: Vec<(Place, Side, PathBuf)>,
    /// A one-item batch (policy or move back) came back stopped.
    engine_stopped: bool,
    landed: Vec<(Side, PathBuf)>,
    versions_used: Vec<VersionsRoot>,
    kept_bytes: u64,
    progress: RunProgress,
}

/// Whether `now` reads exactly as the snapshot's `entry` did. A folder must
/// have the same contents (count, bytes, newest time, held back, unreadable,
/// app-owned, and the name digest); its own time is left out, since browsing
/// it in Finder touches it.
fn matches_snapshot(entry: &Entry, now: Option<&Observed>) -> bool {
    let Some(now) = now else {
        return false;
    };
    if now.facts.kind != entry.facts.kind {
        return false;
    }
    match entry.facts.kind {
        EntryKind::Folder => entry.subtree.is_some() && now.subtree == entry.subtree,
        EntryKind::File | EntryKind::Symlink => {
            now.facts.size == entry.facts.size && now.facts.mtime == entry.facts.mtime
        }
    }
}

fn entry_bytes(entry: &Entry) -> u64 {
    entry
        .subtree
        .map_or_else(|| entry.facts.size.unwrap_or(0), |subtree| subtree.bytes)
}

/// The engine's word on `path`; a source it never answered for in a batch it
/// finished counts as failed, never as done. `None`: the batch stopped first.
fn read(outcome: &BatchOutcome, path: &Path) -> Option<ItemOutcome> {
    let found = outcome.items.iter().find(|(p, _)| p == path).map(|(_, o)| *o);
    match found {
        Some(found) => Some(found),
        None if outcome.stopped => None,
        None => Some(ItemOutcome::Failed),
    }
}

fn result_of(outcome: ItemOutcome) -> RunItemResult {
    match outcome {
        ItemOutcome::Done => RunItemResult::Done,
        ItemOutcome::Skipped => RunItemResult::ChangedSinceCompare,
        ItemOutcome::Failed => RunItemResult::CouldNotDo,
    }
}

fn file_name(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

fn items_of(step: &Step) -> &[u32] {
    match step {
        Step::MakeFolders { .. } => &[],
        Step::KeepAside { items, .. } | Step::Copy { items, .. } | Step::Remove { items, .. } => items,
    }
}

impl<'a> Runner<'a> {
    pub(crate) fn new(
        plan: &'a Plan,
        ctx: &RunContext,
        engine: &'a dyn SyncEngine,
        control: &'a SyncControl,
        on_progress: &'a (dyn Fn(RunProgress) + Sync),
    ) -> Self {
        Self {
            plan,
            ctx: ctx.clone(),
            engine,
            control,
            on_progress,
            acting: BTreeSet::new(),
            results: BTreeMap::new(),
            kept: BTreeMap::new(),
            blocked: Vec::new(),
            holds: None,
            made_for_copies: BTreeSet::new(),
            claimed: Vec::new(),
            keep_dirs: Vec::new(),
            leftover_folders: Vec::new(),
            engine_stopped: false,
            landed: Vec::new(),
            versions_used: Vec::new(),
            kept_bytes: 0,
            progress: RunProgress::default(),
        }
    }

    pub(crate) async fn run(mut self, run: &[u32]) -> RunReport {
        self.acting = self.acting_rows(run);
        let first = steps(self.plan, run, &self.ctx);
        self.start(&first);
        // Cancel lands between batches, and between items while re-checking.
        // TODO(Task 6): forward the cancel into the engine so a long batch
        // stops mid-way; until then a batch that started runs to its end.
        let mut stopped = self.halted() || self.claim(&first.steps).await;
        let plan_steps = steps(self.plan, run, &self.ctx).steps;
        for step in &plan_steps {
            if stopped {
                break;
            }
            self.control.wait_while_paused().await;
            if self.halted() {
                stopped = true;
                break;
            }
            stopped = self.step(step).await || self.halted() || self.engine_stopped;
            self.progress.steps_done += 1;
            (self.on_progress)(self.progress);
        }
        let leftovers: Vec<u32> = self.kept.keys().copied().collect();
        self.restore(&leftovers).await;
        self.tidy().await;
        (self.on_progress)(self.progress);
        self.report(stopped)
    }

    fn report(self, stopped: bool) -> RunReport {
        let results = self
            .acting
            .iter()
            .map(|index| {
                (
                    *index,
                    self.results.get(index).copied().unwrap_or(RunItemResult::NotStarted),
                )
            })
            .collect();
        let empty_folders = self
            .made_for_copies
            .iter()
            .filter(|(side, dir)| !self.landed.iter().any(|(s, path)| s == side && path.starts_with(dir)))
            .cloned()
            .collect();
        RunReport {
            results,
            versions_used: self.versions_used,
            kept_bytes: self.kept_bytes,
            empty_folders,
            leftover_folders: self.leftover_folders,
            stopped,
        }
    }

    fn halted(&self) -> bool {
        self.control.is_cancelled()
    }

    /// Rows of the run that do something, in index order.
    fn acting_rows(&self, run: &[u32]) -> BTreeSet<u32> {
        run.iter()
            .copied()
            .filter(|&index| {
                self.plan
                    .items
                    .get(index as usize)
                    .is_some_and(|item| item.action.target().is_some())
            })
            .collect()
    }

    /// Totals, refusals, and rows the steps couldn't place (never silent).
    fn start(&mut self, plan: &StepPlan) {
        self.progress.steps_total = plan.steps.len() as u32;
        self.progress.items_total = self.acting.len() as u64;
        self.progress.bytes_total = self.acting.iter().map(|&index| self.item(index).bytes).sum();
        for &(index, refusal) in &plan.refused {
            let result = match refusal {
                KeepRefusal::NoTrash => RunItemResult::NoTrash,
                KeepRefusal::EscapesFolder | KeepRefusal::BadTemplate | KeepRefusal::InsideSyncFolder => {
                    RunItemResult::CouldNotDo
                }
            };
            self.settle(index, result);
        }
        let placed: BTreeSet<u32> = plan.steps.iter().flat_map(items_of).copied().collect();
        let unplaced: Vec<u32> = self
            .acting
            .iter()
            .copied()
            .filter(|index| !placed.contains(index) && !self.results.contains_key(index))
            .collect();
        self.settle_all(&unplaced, RunItemResult::CouldNotDo);
        (self.on_progress)(self.progress);
    }

    fn item(&self, index: u32) -> &'a PlanItem {
        &self.plan.items[index as usize]
    }

    /// Sets a row's result; the first one counts toward progress.
    fn settle(&mut self, index: u32, result: RunItemResult) {
        if self.results.insert(index, result).is_none() {
            self.progress.items_done += 1;
            self.progress.bytes_done += self.item(index).bytes;
        }
    }

    fn settle_all(&mut self, items: &[u32], result: RunItemResult) {
        for &index in items {
            self.settle(index, result);
        }
    }

    async fn step(&mut self, step: &Step) -> bool {
        match step {
            Step::MakeFolders { place, side, dirs } => self.make_folders(*place, *side, dirs).await,
            Step::KeepAside { side, items, to } => self.keep_aside(*side, items, to).await,
            Step::Copy { from, dest_dir, items } => self.copy(*from, dest_dir, items).await,
            Step::Remove { side, items, to } => self.remove(*side, items, to).await,
        }
    }

    /// What a blocked folder at or above `dir` means for items there.
    fn blocked_at(&self, place: Place, side: Side, dir: &Path) -> Option<RunItemResult> {
        self.blocked
            .iter()
            .find(|(p, s, d, _)| *p == place && *s == side && dir.starts_with(d))
            .map(|(_, _, _, result)| *result)
    }

    fn path_of(&self, side: Side, entry: &Entry) -> PathBuf {
        self.ctx.root(side).join(&entry.rel_path)
    }

    async fn exists(&self, side: Side, place: Place, path: &Path) -> bool {
        self.engine.observe(side, place, path, false).await.is_some()
    }

    /// After a stopped batch, whether the engine moved `side`'s `entry` to
    /// `to` before it stopped: found at its kept spot, or its place no longer
    /// reads as the snapshot (gone, or something new there).
    async fn went_to(&self, side: Side, entry: &Entry, to: &KeepDestination) -> bool {
        if let KeepDestination::Folder { place, dir } = to {
            let spot = dir.join(file_name(&entry.rel_path));
            let kept = self.engine.observe(side, *place, &spot, false).await;
            if kept.is_some_and(|kept| kept.facts.kind == entry.facts.kind && kept.facts.size == entry.facts.size) {
                return true;
            }
        }
        !self.unchanged(side, entry).await
    }

    async fn unchanged(&self, side: Side, entry: &Entry) -> bool {
        let path = self.path_of(side, entry);
        let folder = entry.facts.kind == EntryKind::Folder;
        let now = self.engine.observe(side, Place::Target, &path, folder).await;
        matches_snapshot(entry, now.as_ref())
    }

    fn bound(&self, side: Side, entry: &Entry) -> BoundSource {
        BoundSource {
            path: self.path_of(side, entry),
            expected: entry.facts,
        }
    }

    async fn make_folders(&mut self, place: Place, side: Side, dirs: &[PathBuf]) -> bool {
        // Never make anything through a folder that was refused.
        let dirs: Vec<PathBuf> = dirs
            .iter()
            .filter(|dir| self.blocked_at(place, side, dir).is_none())
            .cloned()
            .collect();
        if dirs.is_empty() {
            return false;
        }
        let keeps = [self.ctx.staging(side).dir.clone(), self.ctx.versions(side).dir.clone()];
        let for_copies = |dir: &PathBuf| place == Place::Target && !keeps.iter().any(|keep| dir.starts_with(keep));
        // Which target folders are new, so only those count as made by this run.
        let mut absent = BTreeSet::new();
        for dir in dirs.iter().filter(|dir| for_copies(dir)) {
            if !self.exists(side, place, dir).await {
                absent.insert(dir.clone());
            }
        }
        let batch = EngineBatch::MakeFolders {
            place,
            side,
            dirs: dirs.clone(),
        };
        let outcome = self.engine.run(batch).await;
        for dir in dirs {
            if read(&outcome, &dir) != Some(ItemOutcome::Done) {
                self.blocked.push((place, side, dir, RunItemResult::CouldNotDo));
            } else if !for_copies(&dir) {
                self.keep_dirs.push((place, side, dir));
            } else if absent.contains(&dir) {
                self.made_for_copies.insert((side, dir));
            }
        }
        outcome.stopped
    }

    async fn copy(&mut self, from: Side, dest_dir: &Path, items: &[u32]) -> bool {
        let to = other(from);
        if let Some(result) = self.blocked_at(Place::Target, to, dest_dir) {
            let open: Vec<u32> = items
                .iter()
                .copied()
                .filter(|i| !self.results.contains_key(i))
                .collect();
            self.settle_all(&open, result);
            self.restore(items).await;
            return false;
        }
        let mut chosen = Vec::new();
        for &index in items {
            if self.halted() {
                break;
            }
            let item = self.item(index);
            let replaces = item.entry(to).is_some();
            if self.results.contains_key(&index) || (replaces && !self.kept.contains_key(&index)) {
                continue;
            }
            let (Some(source), Some(spot)) = (item.entry(from), landing(self.plan, item, from, &self.ctx)) else {
                self.settle(index, RunItemResult::CouldNotDo);
                continue;
            };
            let at = spot.dest_dir.join(&spot.name);
            if self.exists(to, Place::Target, &at).await || !self.unchanged(from, source).await {
                self.settle(index, RunItemResult::ChangedSinceCompare);
                continue;
            }
            chosen.push((index, self.bound(from, source), at));
        }
        if self.halted() {
            return true;
        }
        let stopped = self.run_copy(from, dest_dir, &chosen).await;
        self.finish_replacements(items).await;
        stopped
    }

    async fn run_copy(&mut self, from: Side, dest_dir: &Path, chosen: &[(u32, BoundSource, PathBuf)]) -> bool {
        if chosen.is_empty() {
            return false;
        }
        let to = other(from);
        let batch = EngineBatch::Copy {
            from,
            sources: chosen.iter().map(|(_, source, _)| source.clone()).collect(),
            dest_dir: dest_dir.to_path_buf(),
        };
        let outcome = self.engine.run(batch).await;
        for (index, source, at) in chosen {
            // A stopped batch may still have landed this one (temp+rename).
            let done = match read(&outcome, &source.path) {
                Some(done) => done,
                None if self.exists(to, Place::Target, at).await => ItemOutcome::Done,
                None => continue,
            };
            if done == ItemOutcome::Done {
                self.landed.push((to, at.clone()));
            }
            self.settle(*index, result_of(done));
        }
        outcome.stopped
    }

    async fn remove(&mut self, side: Side, items: &[u32], to: &KeepDestination) -> bool {
        let refused = match to {
            KeepDestination::Folder { place, dir } => self.blocked_at(*place, side, dir),
            KeepDestination::Trash => None,
            KeepDestination::Refused(_) => Some(RunItemResult::CouldNotDo),
        };
        if let Some(result) = refused {
            self.settle_all(items, result);
            return false;
        }
        let held = match self.holds.take() {
            Some(held) => held,
            None => self.unlanded_copies(),
        };
        let mut chosen = Vec::new();
        for &index in items {
            if self.halted() {
                break;
            }
            if let Some(result) = self.removal_check(index, side, to, &held).await {
                self.settle(index, result);
            } else if let Some(entry) = self.item(index).entry(side) {
                chosen.push((index, self.bound(side, entry)));
            }
        }
        self.holds = Some(held);
        if self.halted() {
            return true;
        }
        self.run_removal(side, to, &chosen).await
    }

    /// Why a removal can't go ahead, if it can't.
    async fn removal_check(
        &self,
        index: u32,
        side: Side,
        to: &KeepDestination,
        held: &[holds::Hold],
    ) -> Option<RunItemResult> {
        let item = self.item(index);
        let Some(entry) = item.entry(side) else {
            return Some(RunItemResult::CouldNotDo);
        };
        if held.iter().any(|hold| hold.holds(side, &item.key, entry)) {
            return Some(RunItemResult::HeldAfterFailedCopy);
        }
        if !self.unchanged(side, entry).await {
            return Some(RunItemResult::ChangedSinceCompare);
        }
        if let KeepDestination::Folder { place, dir } = to
            && self.exists(side, *place, &dir.join(file_name(&entry.rel_path))).await
        {
            return Some(RunItemResult::VersionsSpotTaken);
        }
        None
    }

    async fn run_removal(&mut self, side: Side, to: &KeepDestination, chosen: &[(u32, BoundSource)]) -> bool {
        let Some(outcome) = self.run_keep(side, to, chosen).await else {
            return false;
        };
        for (index, source) in chosen {
            let done = match read(&outcome, &source.path) {
                Some(done) => done,
                None => match self.item(*index).entry(side) {
                    Some(entry) if self.went_to(side, entry, to).await => ItemOutcome::Done,
                    _ => continue,
                },
            };
            if let (ItemOutcome::Done, KeepDestination::Folder { .. }, Some(entry)) =
                (done, to, self.item(*index).entry(side))
            {
                let versions = self.ctx.versions(side).clone();
                self.note_kept(versions, entry_bytes(entry));
            }
            self.settle(*index, result_of(done));
        }
        outcome.stopped
    }

    /// Runs the batch that keeps `chosen` at `to`; `None` when there is nothing
    /// to run.
    async fn run_keep(&self, side: Side, to: &KeepDestination, chosen: &[(u32, BoundSource)]) -> Option<BatchOutcome> {
        if chosen.is_empty() {
            return None;
        }
        let sources = chosen.iter().map(|(_, source)| source.clone()).collect();
        let batch = match to {
            KeepDestination::Trash => EngineBatch::Trash { side, sources },
            KeepDestination::Folder { place, dir } => EngineBatch::Move {
                side,
                sources,
                place: *place,
                dest_dir: dir.clone(),
            },
            KeepDestination::Refused(_) => return None,
        };
        Some(self.engine.run(batch).await)
    }

    fn note_kept(&mut self, root: VersionsRoot, bytes: u64) {
        if !self.versions_used.contains(&root) {
            self.versions_used.push(root);
        }
        self.kept_bytes += bytes;
    }
}
