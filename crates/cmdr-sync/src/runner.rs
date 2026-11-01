//! The state machine behind `execute`: one step at a time, every item
//! re-checked against the snapshot right before its batch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::steps::{landing, other};
use crate::{
    BatchOutcome, BoundSource, EngineBatch, Entry, EntryKey, EntryKind, ItemOutcome, KeepDestination, KeepRefusal,
    Observed, Place, Plan, PlanItem, RunContext, RunItemResult, RunProgress, RunReport, Side, Step, SyncControl,
    SyncEngine, VersionsRoot, steps,
};

pub(crate) struct Runner<'a> {
    plan: &'a Plan,
    ctx: &'a RunContext,
    engine: &'a dyn SyncEngine,
    control: &'a SyncControl,
    on_progress: &'a (dyn Fn(RunProgress) + Sync),
    results: BTreeMap<u32, RunItemResult>,
    /// Replacement targets set aside, and where they went.
    kept: BTreeMap<u32, KeepDestination>,
    failed_dirs: Vec<(Place, Side, PathBuf)>,
    /// Folders (per side) where a copy failed: removals beneath are held.
    held_under: Vec<(Side, EntryKey)>,
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
/// finished counts as failed, never as done.
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

impl<'a> Runner<'a> {
    pub(crate) fn new(
        plan: &'a Plan,
        ctx: &'a RunContext,
        engine: &'a dyn SyncEngine,
        control: &'a SyncControl,
        on_progress: &'a (dyn Fn(RunProgress) + Sync),
    ) -> Self {
        Self {
            plan,
            ctx,
            engine,
            control,
            on_progress,
            results: BTreeMap::new(),
            kept: BTreeMap::new(),
            failed_dirs: Vec::new(),
            held_under: Vec::new(),
            versions_used: Vec::new(),
            kept_bytes: 0,
            progress: RunProgress::default(),
        }
    }

    pub(crate) async fn run(mut self, run: &[u32]) -> RunReport {
        let step_plan = steps(self.plan, run, self.ctx);
        let acting = self.acting(run);
        self.start(&acting, &step_plan.steps, &step_plan.refused);
        let mut stopped = false;
        for step in &step_plan.steps {
            self.control.wait_while_paused().await;
            if self.control.is_cancelled() {
                stopped = true;
                break;
            }
            let engine_stopped = self.step(step).await;
            self.progress.steps_done += 1;
            (self.on_progress)(self.progress);
            if engine_stopped {
                stopped = true;
                break;
            }
        }
        let leftovers: Vec<u32> = self.kept.keys().copied().collect();
        self.restore(&leftovers).await;
        (self.on_progress)(self.progress);
        let results = acting
            .iter()
            .map(|index| {
                (
                    *index,
                    self.results.get(index).copied().unwrap_or(RunItemResult::NotStarted),
                )
            })
            .collect();
        RunReport {
            results,
            versions_used: self.versions_used,
            kept_bytes: self.kept_bytes,
            stopped,
        }
    }

    /// Rows of the run that do something, in index order.
    fn acting(&self, run: &[u32]) -> BTreeSet<u32> {
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
    fn start(&mut self, acting: &BTreeSet<u32>, steps: &[Step], refused: &[(u32, KeepRefusal)]) {
        self.progress.steps_total = steps.len() as u32;
        self.progress.items_total = acting.len() as u64;
        self.progress.bytes_total = acting.iter().map(|&index| self.item(index).bytes).sum();
        for &(index, refusal) in refused {
            let result = match refusal {
                KeepRefusal::NoTrash => RunItemResult::NoTrash,
                KeepRefusal::EscapesFolder | KeepRefusal::BadTemplate => RunItemResult::CouldNotDo,
            };
            self.settle(index, result);
        }
        let placed: BTreeSet<u32> = steps
            .iter()
            .flat_map(|step| match step {
                Step::MakeFolders { .. } => Vec::new(),
                Step::KeepAside { items, .. } | Step::Copy { items, .. } | Step::Remove { items, .. } => items.clone(),
            })
            .collect();
        for &index in acting {
            if !placed.contains(&index) && !self.results.contains_key(&index) {
                self.settle(index, RunItemResult::CouldNotDo);
            }
        }
        (self.on_progress)(self.progress);
    }

    fn item(&self, index: u32) -> &'a PlanItem {
        &self.plan.items[index as usize]
    }

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

    fn blocked(&self, place: Place, side: Side, dir: &Path) -> bool {
        self.failed_dirs
            .iter()
            .any(|(p, s, d)| *p == place && *s == side && dir.starts_with(d))
    }

    fn path_of(&self, side: Side, entry: &Entry) -> PathBuf {
        self.ctx.root(side).join(&entry.rel_path)
    }

    async fn unchanged(&self, side: Side, entry: &Entry) -> bool {
        let path = self.path_of(side, entry);
        let folder = entry.facts.kind == EntryKind::Folder;
        let now = self.engine.observe(side, &path, folder).await;
        matches_snapshot(entry, now.as_ref())
    }

    fn bound(&self, side: Side, entry: &Entry) -> BoundSource {
        BoundSource {
            path: self.path_of(side, entry),
            expected: entry.facts,
        }
    }

    fn note_kept(&mut self, side: Side, entry: &Entry) {
        let versions = self.ctx.versions(side);
        if !self.versions_used.contains(versions) {
            self.versions_used.push(versions.clone());
        }
        self.kept_bytes += entry_bytes(entry);
    }

    async fn make_folders(&mut self, place: Place, side: Side, dirs: &[PathBuf]) -> bool {
        let batch = EngineBatch::MakeFolders {
            place,
            side,
            dirs: dirs.to_vec(),
        };
        let outcome = self.engine.run(batch).await;
        for dir in dirs {
            if read(&outcome, dir) != Some(ItemOutcome::Done) {
                self.failed_dirs.push((place, side, dir.clone()));
            }
        }
        outcome.stopped
    }

    /// The batch that keeps `sources` at `to`, or `None` when it can't run.
    fn keep_batch(&self, side: Side, to: &KeepDestination, sources: Vec<BoundSource>) -> Option<EngineBatch> {
        match to {
            KeepDestination::Trash => Some(EngineBatch::Trash { side, sources }),
            KeepDestination::Folder { place, dir } if !self.blocked(*place, side, dir) => Some(EngineBatch::Move {
                side,
                sources,
                place: *place,
                dest_dir: dir.clone(),
            }),
            KeepDestination::Folder { .. } | KeepDestination::Refused(_) => None,
        }
    }

    async fn keep_aside(&mut self, side: Side, items: &[u32], to: &KeepDestination) -> bool {
        if self.keep_batch(side, to, Vec::new()).is_none() {
            self.settle_all(items, RunItemResult::CouldNotDo);
            return false;
        }
        let mut chosen = Vec::new();
        for &index in items {
            let item = self.item(index);
            let (Some(old), Some(source)) = (item.entry(side), item.entry(other(side))) else {
                self.settle(index, RunItemResult::CouldNotDo);
                continue;
            };
            // A copy that can't run must not cost the target its file.
            if !self.unchanged(other(side), source).await || !self.unchanged(side, old).await {
                self.settle(index, RunItemResult::ChangedSinceCompare);
                continue;
            }
            chosen.push((index, self.bound(side, old)));
        }
        let Some(outcome) = self.run_keep(side, to, &chosen).await else {
            return false;
        };
        for (index, source) in &chosen {
            match read(&outcome, &source.path) {
                Some(ItemOutcome::Done) => {
                    self.kept.insert(*index, to.clone());
                    if let (KeepDestination::Folder { .. }, Some(old)) = (to, self.item(*index).entry(side)) {
                        self.note_kept(side, old);
                    }
                }
                Some(other) => self.settle(*index, result_of(other)),
                None => {}
            }
        }
        outcome.stopped
    }

    async fn run_keep(&self, side: Side, to: &KeepDestination, chosen: &[(u32, BoundSource)]) -> Option<BatchOutcome> {
        if chosen.is_empty() {
            return None;
        }
        let sources = chosen.iter().map(|(_, source)| source.clone()).collect();
        let batch = self.keep_batch(side, to, sources)?;
        Some(self.engine.run(batch).await)
    }

    async fn copy(&mut self, from: Side, dest_dir: &Path, items: &[u32]) -> bool {
        let to = other(from);
        if self.blocked(Place::Target, to, dest_dir) {
            self.settle_all(items, RunItemResult::CouldNotDo);
            return false;
        }
        let mut chosen = Vec::new();
        for &index in items {
            let item = self.item(index);
            let replaces = item.entry(to).is_some();
            if self.results.contains_key(&index) || (replaces && !self.kept.contains_key(&index)) {
                continue;
            }
            let (Some(source), Some(spot)) = (item.entry(from), landing(self.plan, item, from, self.ctx)) else {
                self.settle(index, RunItemResult::CouldNotDo);
                continue;
            };
            let occupied = self
                .engine
                .observe(to, &spot.dest_dir.join(&spot.name), false)
                .await
                .is_some();
            if occupied || !self.unchanged(from, source).await {
                self.settle(index, RunItemResult::ChangedSinceCompare);
                continue;
            }
            chosen.push((index, self.bound(from, source)));
        }
        let stopped = self.run_copy(from, dest_dir, &chosen).await;
        self.restore(items).await;
        stopped
    }

    async fn run_copy(&mut self, from: Side, dest_dir: &Path, chosen: &[(u32, BoundSource)]) -> bool {
        if chosen.is_empty() {
            return false;
        }
        let batch = EngineBatch::Copy {
            from,
            sources: chosen.iter().map(|(_, source)| source.clone()).collect(),
            dest_dir: dest_dir.to_path_buf(),
        };
        let outcome = self.engine.run(batch).await;
        for (index, source) in chosen {
            let Some(done) = read(&outcome, &source.path) else {
                continue;
            };
            if done == ItemOutcome::Failed {
                let folder = self.item(*index).key.parent().unwrap_or_else(EntryKey::root);
                self.held_under.push((other(from), folder));
            }
            self.settle(*index, result_of(done));
        }
        outcome.stopped
    }

    /// Puts back what was set aside for these rows' copies, unless the copy
    /// landed. From the Trash nothing can come back: `KeptNotReplaced`.
    async fn restore(&mut self, items: &[u32]) {
        for &index in items {
            if self.results.get(&index) == Some(&RunItemResult::Done) {
                self.kept.remove(&index);
                continue;
            }
            let Some(to) = self.kept.remove(&index) else {
                continue;
            };
            let back = match to {
                KeepDestination::Folder { place, dir } => self.move_back(index, place, &dir).await,
                KeepDestination::Trash | KeepDestination::Refused(_) => false,
            };
            if !back {
                self.settle(index, RunItemResult::KeptNotReplaced);
            }
        }
    }

    async fn move_back(&mut self, index: u32, place: Place, dir: &Path) -> bool {
        let item = self.item(index);
        let Some(side) = item.action.target() else {
            return false;
        };
        let Some(old) = item.entry(side) else {
            return false;
        };
        let original = self.path_of(side, old);
        let Some(parent) = original.parent() else {
            return false;
        };
        if self.engine.observe(side, &original, false).await.is_some() {
            return false;
        }
        let kept_at = dir.join(file_name(&old.rel_path));
        let batch = EngineBatch::MoveBack {
            side,
            place,
            sources: vec![BoundSource {
                path: kept_at.clone(),
                expected: old.facts,
            }],
            dest_dir: parent.to_path_buf(),
        };
        let outcome = self.engine.run(batch).await;
        let back = read(&outcome, &kept_at) == Some(ItemOutcome::Done);
        if back {
            self.kept_bytes = self.kept_bytes.saturating_sub(entry_bytes(old));
        }
        back
    }

    async fn remove(&mut self, side: Side, items: &[u32], to: &KeepDestination) -> bool {
        if self.keep_batch(side, to, Vec::new()).is_none() {
            self.settle_all(items, RunItemResult::CouldNotDo);
            return false;
        }
        let mut chosen = Vec::new();
        for &index in items {
            let item = self.item(index);
            let held = self
                .held_under
                .iter()
                .any(|(s, folder)| *s == side && item.key.is_within(folder));
            let Some(entry) = item.entry(side) else {
                self.settle(index, RunItemResult::CouldNotDo);
                continue;
            };
            if held {
                self.settle(index, RunItemResult::HeldAfterFailedCopy);
            } else if !self.unchanged(side, entry).await {
                self.settle(index, RunItemResult::ChangedSinceCompare);
            } else {
                chosen.push((index, self.bound(side, entry)));
            }
        }
        let Some(outcome) = self.run_keep(side, to, &chosen).await else {
            return false;
        };
        for (index, source) in &chosen {
            let Some(done) = read(&outcome, &source.path) else {
                continue;
            };
            if let (ItemOutcome::Done, KeepDestination::Folder { .. }, Some(entry)) =
                (done, to, self.item(*index).entry(side))
            {
                self.note_kept(side, entry);
            }
            self.settle(*index, result_of(done));
        }
        outcome.stopped
    }
}
