//! A `SyncEngine` double for the executor tests (test-only).
//!
//! It models both roots and the local disk as path → facts maps, records every
//! batch, and acts blindly, as a careless engine would. Instead of protecting
//! anything itself it writes down every violation of the executor's promises:
//! a source moved, trashed, or copied while it (or anything beneath it) differs
//! from the snapshot, a landing that would overwrite, a kept item restored onto
//! something. Tests script failures, a stopped batch, changes between batches,
//! and a cancel after a given batch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::*;

/// Where a path lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Loc {
    Left,
    Right,
    Local,
}

pub(crate) fn loc_of(side: Side) -> Loc {
    match side {
        Side::Left => Loc::Left,
        Side::Right => Loc::Right,
    }
}

fn loc_at(place: Place, side: Side) -> Loc {
    match place {
        Place::Target => loc_of(side),
        Place::LocalDisk => Loc::Local,
    }
}

/// Something that happens to a volume while the run is going on.
#[derive(Debug, Clone)]
pub(crate) enum Change {
    /// The item now reads like this (added or edited).
    Set(Loc, PathBuf, Observed),
    /// The item is gone.
    Remove(Loc, PathBuf),
}

type State = BTreeMap<(Loc, PathBuf), Observed>;

#[derive(Default)]
struct Inner {
    state: State,
    snapshot: State,
    batches: Vec<EngineBatch>,
    violations: Vec<String>,
    trashed: Vec<(Loc, PathBuf)>,
    trashed_facts: Vec<Observed>,
    /// Where the run moved things: sources there are kept items, not
    /// snapshot entries.
    kept_paths: BTreeSet<(Loc, PathBuf)>,
    /// Snapshot paths the run moved or trashed and hasn't moved back.
    away: BTreeSet<(Loc, PathBuf)>,
    /// Removed by the outside world, not by the run.
    gone: Vec<(Loc, PathBuf)>,
}

/// The double. Script it before the run, read it after.
#[derive(Default)]
pub(crate) struct FakeEngine {
    inner: Mutex<Inner>,
    fail: BTreeSet<PathBuf>,
    skip: BTreeSet<PathBuf>,
    stop_at: Option<usize>,
    stop_after_work: Option<usize>,
    cancel_on_observe: Option<(PathBuf, Arc<SyncControl>)>,
    changes_after: BTreeMap<usize, Vec<Change>>,
    cancel_after: Option<(usize, Arc<SyncControl>)>,
}

fn observed_of(entry: &Entry) -> Observed {
    Observed {
        facts: entry.facts,
        subtree: entry.subtree,
    }
}

fn beneath<'a>(state: &'a State, loc: Loc, path: &'a Path) -> impl Iterator<Item = (&'a (Loc, PathBuf), &'a Observed)> {
    state
        .range((loc, path.to_path_buf())..)
        .take_while(move |((l, p), _)| *l == loc && p.starts_with(path))
}

/// What a real digest does: an edit anywhere shows in every folder above it.
fn touch_ancestors(state: &mut State, loc: Loc, path: &Path) {
    for ancestor in path.ancestors().skip(1) {
        if let Some(folder) = state.get_mut(&(loc, ancestor.to_path_buf()))
            && let Some(subtree) = folder.subtree.as_mut()
        {
            subtree.digest = subtree.digest.wrapping_add(1);
        }
    }
}

impl FakeEngine {
    /// Both sides as the snapshots saw them, under the run's roots.
    pub(crate) fn new(left: &Snapshot, right: &Snapshot, ctx: &RunContext) -> Self {
        let mut state = State::new();
        for (loc, snap, root) in [(Loc::Left, left, &ctx.left_root), (Loc::Right, right, &ctx.right_root)] {
            state.insert(
                (loc, root.clone()),
                Observed {
                    facts: EntryFacts {
                        kind: EntryKind::Folder,
                        size: None,
                        mtime: None,
                    },
                    subtree: None,
                },
            );
            for entry in snap.entries.values() {
                state.insert((loc, root.join(&entry.rel_path)), observed_of(entry));
            }
        }
        Self {
            inner: Mutex::new(Inner {
                snapshot: state.clone(),
                state,
                ..Inner::default()
            }),
            ..Self::default()
        }
    }

    /// The engine answers `Failed` for this source path (or folder to make).
    pub(crate) fn failing(mut self, path: impl Into<PathBuf>) -> Self {
        self.fail.insert(path.into());
        self
    }

    /// The engine answers `Skipped` for this source path.
    pub(crate) fn skipping(mut self, path: impl Into<PathBuf>) -> Self {
        self.skip.insert(path.into());
        self
    }

    /// Batch `n` (0-based) is stopped by the engine before doing anything.
    pub(crate) fn stopping_at(mut self, n: usize) -> Self {
        self.stop_at = Some(n);
        self
    }

    /// Batch `n` (0-based) does its work, then reports stopped with no
    /// answers, as an engine cancelled mid-report would.
    pub(crate) fn stopping_after_work(mut self, n: usize) -> Self {
        self.stop_after_work = Some(n);
        self
    }

    /// The person presses Cancel while the executor looks at `path`.
    pub(crate) fn cancel_on_observe(mut self, path: impl Into<PathBuf>, control: Arc<SyncControl>) -> Self {
        self.cancel_on_observe = Some((path.into(), control));
        self
    }

    /// `change` happens right after batch `n` (0-based) finishes.
    pub(crate) fn after_batch(mut self, n: usize, change: Change) -> Self {
        self.changes_after.entry(n).or_default().push(change);
        self
    }

    /// The person presses Cancel right after batch `n`.
    pub(crate) fn cancel_after(mut self, n: usize, control: Arc<SyncControl>) -> Self {
        self.cancel_after = Some((n, control));
        self
    }

    /// `change` has already happened before the run starts.
    pub(crate) fn changed(self, change: Change) -> Self {
        self.lock().apply(change);
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn batches(&self) -> Vec<EngineBatch> {
        self.lock().batches.clone()
    }

    pub(crate) fn violations(&self) -> Vec<String> {
        self.lock().violations.clone()
    }

    /// Whether the run moved or trashed this snapshot path and never put it
    /// back.
    pub(crate) fn moved_away(&self, loc: Loc, path: impl AsRef<Path>) -> bool {
        self.lock().away.contains(&(loc, path.as_ref().to_path_buf()))
    }

    pub(crate) fn trashed(&self) -> Vec<(Loc, PathBuf)> {
        self.lock().trashed.clone()
    }

    pub(crate) fn get(&self, loc: Loc, path: impl AsRef<Path>) -> Option<Observed> {
        self.lock().state.get(&(loc, path.as_ref().to_path_buf())).copied()
    }

    /// What a real volume would say: a folder's digest follows what is
    /// beneath it now, whatever was stored for it.
    fn read_now(&self, loc: Loc, path: &Path) -> Option<Observed> {
        let inner = self.lock();
        let mut now = *inner.state.get(&(loc, path.to_path_buf()))?;
        if let Some(subtree) = now.subtree.as_mut() {
            let below = |state: &State| -> Vec<((Loc, PathBuf), Observed)> {
                beneath(state, loc, path)
                    .filter(|((_, p), _)| p != path)
                    .map(|(k, o)| (k.clone(), *o))
                    .collect()
            };
            if below(&inner.state) != below(&inner.snapshot) {
                subtree.digest = subtree.digest.wrapping_add(0x9e37_79b9);
            }
        }
        Some(now)
    }

    /// Snapshot items that are no longer anywhere: not in place, not moved,
    /// not in the Trash. With no delete in the engine this must stay empty.
    pub(crate) fn lost(&self) -> Vec<(Loc, PathBuf)> {
        let inner = self.lock();
        let kept_somewhere =
            |facts: &Observed| inner.state.values().chain(&inner.trashed_facts).any(|now| now == facts);
        inner
            .snapshot
            .iter()
            .filter(|(key, _)| !inner.state.contains_key(key))
            .filter(|((loc, path), _)| {
                !inner
                    .trashed
                    .iter()
                    .chain(&inner.gone)
                    .any(|(tl, tp)| tl == loc && path.starts_with(tp))
            })
            .filter(|(_, facts)| !kept_somewhere(facts))
            .map(|(key, _)| key.clone())
            .collect()
    }

    fn outcome(&self, path: &Path) -> Option<ItemOutcome> {
        if self.fail.contains(path) {
            Some(ItemOutcome::Failed)
        } else if self.skip.contains(path) {
            Some(ItemOutcome::Skipped)
        } else {
            None
        }
    }

    fn execute(&self, batch: &EngineBatch) -> Vec<(PathBuf, ItemOutcome)> {
        let mut inner = self.lock();
        match batch {
            EngineBatch::RemoveEmptyFolders { place, side, dirs } => dirs
                .iter()
                .map(|dir| (dir.clone(), inner.remove_empty(loc_at(*place, *side), dir)))
                .collect(),
            EngineBatch::ClaimFolder { place, side, dir } => {
                let outcome = self
                    .outcome(dir)
                    .unwrap_or_else(|| inner.claim(loc_at(*place, *side), dir));
                vec![(dir.clone(), outcome)]
            }
            EngineBatch::MakeFolders { place, side, dirs } => dirs
                .iter()
                .map(|dir| {
                    let outcome = self
                        .outcome(dir)
                        .unwrap_or_else(|| inner.make_folder(loc_at(*place, *side), dir));
                    (dir.clone(), outcome)
                })
                .collect(),
            EngineBatch::Copy {
                from,
                sources,
                dest_dir,
            } => sources
                .iter()
                .map(|source| {
                    let outcome = self
                        .outcome(&source.path)
                        .unwrap_or_else(|| inner.copy(loc_of(*from), source, dest_dir));
                    (source.path.clone(), outcome)
                })
                .collect(),
            EngineBatch::Move {
                side,
                sources,
                place,
                dest_dir,
            } => sources
                .iter()
                .map(|source| {
                    let outcome = self.outcome(&source.path).unwrap_or_else(|| {
                        inner.check_snapshot(loc_of(*side), source, "moved");
                        inner.relocate(loc_of(*side), &source.path, loc_at(*place, *side), dest_dir)
                    });
                    (source.path.clone(), outcome)
                })
                .collect(),
            EngineBatch::MoveBack {
                side,
                place,
                sources,
                dest_dir,
            } => sources
                .iter()
                .map(|source| {
                    let outcome = self
                        .outcome(&source.path)
                        .unwrap_or_else(|| inner.move_back(loc_at(*place, *side), loc_of(*side), source, dest_dir));
                    (source.path.clone(), outcome)
                })
                .collect(),
            EngineBatch::Trash { side, sources } => sources
                .iter()
                .map(|source| {
                    let outcome = self.outcome(&source.path).unwrap_or_else(|| {
                        let loc = loc_of(*side);
                        inner.check_snapshot(loc, source, "trashed");
                        let gone: Vec<Observed> = beneath(&inner.state, loc, &source.path).map(|(_, o)| *o).collect();
                        inner.trashed_facts.extend(gone);
                        inner
                            .state
                            .retain(|(l, p), _| !(*l == loc && p.starts_with(&source.path)));
                        inner.trashed.push((loc, source.path.clone()));
                        if !inner.kept_paths.contains(&(loc, source.path.clone())) {
                            inner.away.insert((loc, source.path.clone()));
                        }
                        ItemOutcome::Done
                    });
                    (source.path.clone(), outcome)
                })
                .collect(),
        }
    }
}

impl Inner {
    fn apply(&mut self, change: Change) {
        match change {
            Change::Set(loc, path, observed) => {
                touch_ancestors(&mut self.state, loc, &path);
                self.state.insert((loc, path), observed);
            }
            Change::Remove(loc, path) => {
                touch_ancestors(&mut self.state, loc, &path);
                self.gone.push((loc, path.clone()));
                self.state.retain(|(l, p), _| !(*l == loc && p.starts_with(&path)));
            }
        }
    }

    /// A source must read exactly as the snapshot did, all the way down, with
    /// nothing added beneath it, and the binding must carry those facts.
    fn check_snapshot(&mut self, loc: Loc, source: &BoundSource, verb: &str) {
        if self.kept_paths.contains(&(loc, source.path.clone())) {
            let now = self.state.get(&(loc, source.path.clone())).map(|o| o.facts);
            if now != Some(source.expected) {
                self.violations
                    .push(format!("{verb} kept {:?} that isn't the kept item", source.path));
            }
            return;
        }
        let now: Vec<_> = beneath(&self.state, loc, &source.path).collect();
        let then: Vec<_> = beneath(&self.snapshot, loc, &source.path).collect();
        if now.is_empty() || now != then {
            self.violations
                .push(format!("{verb} {:?} that differs from the snapshot", source.path));
        }
        let snap = self.snapshot.get(&(loc, source.path.clone()));
        if snap.is_none_or(|snap| snap.facts != source.expected) {
            self.violations.push(format!(
                "{verb} {:?} bound to facts the snapshot never had",
                source.path
            ));
        }
    }

    /// `mkdir -p`: through real folders only.
    fn make_folder(&mut self, loc: Loc, dir: &Path) -> ItemOutcome {
        let mut parents: Vec<&Path> = dir.ancestors().skip(1).collect();
        parents.reverse();
        for parent in parents {
            if let Some(existing) = self.state.get(&(loc, parent.to_path_buf())) {
                if existing.facts.kind != EntryKind::Folder {
                    return ItemOutcome::Failed;
                }
            } else if parent.parent().is_some() {
                self.insert_folder(loc, parent);
            }
        }
        match self.state.get(&(loc, dir.to_path_buf())) {
            Some(existing) if existing.facts.kind != EntryKind::Folder => ItemOutcome::Failed,
            Some(_) => ItemOutcome::Done,
            None => {
                self.insert_folder(loc, dir);
                ItemOutcome::Done
            }
        }
    }

    /// `rmdir`: only an empty folder goes; never one the snapshot listed.
    fn remove_empty(&mut self, loc: Loc, dir: &Path) -> ItemOutcome {
        let key = (loc, dir.to_path_buf());
        if !self.state.contains_key(&key) {
            return ItemOutcome::Skipped;
        }
        if self.snapshot.contains_key(&key) {
            self.violations.push(format!("removed snapshot folder {dir:?}"));
            return ItemOutcome::Failed;
        }
        if beneath(&self.state, loc, dir).any(|((_, p), _)| p != dir) {
            return ItemOutcome::Failed;
        }
        self.state.remove(&key);
        ItemOutcome::Done
    }

    fn claim(&mut self, loc: Loc, dir: &Path) -> ItemOutcome {
        if self.state.contains_key(&(loc, dir.to_path_buf())) {
            ItemOutcome::Skipped
        } else {
            self.make_folder(loc, dir)
        }
    }

    fn insert_folder(&mut self, loc: Loc, dir: &Path) {
        let folder = Observed {
            facts: EntryFacts {
                kind: EntryKind::Folder,
                size: None,
                mtime: None,
            },
            subtree: Some(SubtreeFacts::default()),
        };
        self.state.insert((loc, dir.to_path_buf()), folder);
    }

    fn copy(&mut self, from: Loc, source: &BoundSource, dest_dir: &Path) -> ItemOutcome {
        self.check_snapshot(from, source, "copied");
        let to = match from {
            Loc::Left => Loc::Right,
            _ => Loc::Left,
        };
        let Some(name) = source.path.file_name() else {
            return ItemOutcome::Failed;
        };
        let dest = dest_dir.join(name);
        if self.state.contains_key(&(to, dest.clone())) {
            self.violations.push(format!("copy onto existing {dest:?}"));
            return ItemOutcome::Skipped;
        }
        let copied: Vec<_> = beneath(&self.state, from, &source.path)
            .map(|((_, path), observed)| (path.clone(), *observed))
            .collect();
        for (path, observed) in copied {
            let rel = path.strip_prefix(&source.path).unwrap_or(Path::new(""));
            let landed = if rel.as_os_str().is_empty() {
                dest.clone()
            } else {
                dest.join(rel)
            };
            self.state.insert((to, landed), observed);
        }
        ItemOutcome::Done
    }

    fn relocate(&mut self, from: Loc, path: &Path, to: Loc, dest_dir: &Path) -> ItemOutcome {
        let Some(name) = path.file_name() else {
            return ItemOutcome::Failed;
        };
        let dest = dest_dir.join(name);
        if self.state.contains_key(&(to, dest.clone())) {
            self.violations.push(format!("move onto existing {dest:?}"));
            return ItemOutcome::Skipped;
        }
        let moved: Vec<_> = beneath(&self.state, from, path)
            .map(|((_, p), observed)| (p.clone(), *observed))
            .collect();
        if moved.is_empty() {
            return ItemOutcome::Failed;
        }
        if !self.kept_paths.contains(&(from, path.to_path_buf())) {
            self.away.insert((from, path.to_path_buf()));
        }
        self.away.remove(&(to, dest.clone()));
        self.kept_paths.insert((to, dest.clone()));
        for (old, observed) in moved {
            self.state.remove(&(from, old.clone()));
            let rel = old.strip_prefix(path).unwrap_or(Path::new(""));
            let landed = if rel.as_os_str().is_empty() {
                dest.clone()
            } else {
                dest.join(rel)
            };
            self.state.insert((to, landed), observed);
        }
        ItemOutcome::Done
    }

    fn move_back(&mut self, from: Loc, to: Loc, source: &BoundSource, dest_dir: &Path) -> ItemOutcome {
        let now = self.state.get(&(from, source.path.clone())).map(|o| o.facts);
        if now != Some(source.expected) {
            self.violations
                .push(format!("moved back {:?} that isn't the kept item", source.path));
        }
        self.relocate(from, &source.path, to, dest_dir)
    }
}

impl SyncEngine for FakeEngine {
    fn observe<'a>(
        &'a self,
        side: Side,
        place: Place,
        path: &'a Path,
        with_subtree: bool,
    ) -> EngineFuture<'a, Option<Observed>> {
        Box::pin(async move {
            if let Some((at, control)) = &self.cancel_on_observe
                && at == path
            {
                control.cancel();
            }
            self.read_now(loc_at(place, side), path).map(|observed| Observed {
                subtree: if with_subtree { observed.subtree } else { None },
                ..observed
            })
        })
    }

    fn run<'a>(&'a self, batch: EngineBatch) -> EngineFuture<'a, BatchOutcome> {
        Box::pin(async move {
            let n = {
                let mut inner = self.lock();
                inner.batches.push(batch.clone());
                inner.batches.len() - 1
            };
            let outcome = if self.stop_at == Some(n) {
                BatchOutcome {
                    items: Vec::new(),
                    stopped: true,
                }
            } else if self.stop_after_work == Some(n) {
                self.execute(&batch);
                BatchOutcome {
                    items: Vec::new(),
                    stopped: true,
                }
            } else {
                BatchOutcome {
                    items: self.execute(&batch),
                    stopped: false,
                }
            };
            for change in self.changes_after.get(&n).into_iter().flatten() {
                self.lock().apply(change.clone());
            }
            if let Some((after, control)) = &self.cancel_after
                && *after == n
            {
                control.cancel();
            }
            outcome
        })
    }
}
