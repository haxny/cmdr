//! Claiming the dated versions folders, setting replaced files aside, and
//! what happens to them once their copy has (or hasn't) landed.

use std::path::Path;

use super::{Runner, entry_bytes, file_name, read, result_of};
use crate::steps::other;
use crate::{
    BoundSource, EngineBatch, EntryKind, ItemOutcome, KeepDestination, Place, RunItemResult, Side, Step, VersionsRoot,
    keep_destination,
};

/// Dated folders tried per side before giving up (`<stamp> (2)` and on).
const MAX_ATTEMPTS: u32 = 100;

/// How claiming one side's dated folders went.
enum Claim {
    /// All made fresh by this run.
    Made,
    /// One already existed: try the next attempt.
    Taken,
    /// Can't be used; everything kept there gets this result.
    Refused(VersionsRoot, RunItemResult),
    /// The engine stopped.
    Stopped,
}

/// Which of `ctx`'s dated folders `wanted` (staging, versions), once each.
fn roots_of(staging: &VersionsRoot, versions: &VersionsRoot, wanted: (bool, bool)) -> Vec<VersionsRoot> {
    let mut roots = Vec::new();
    for (root, want) in [(staging, wanted.0), (versions, wanted.1)] {
        if want && !roots.contains(root) {
            roots.push(root.clone());
        }
    }
    roots
}

impl Runner<'_> {
    /// Whether `steps` keep anything in `side`'s staging and versions folders.
    fn wanted(&self, steps: &[Step], side: Side) -> (bool, bool) {
        let uses = |root: &VersionsRoot| {
            steps.iter().any(|step| {
                matches!(step, Step::MakeFolders { place, side: s, dirs }
                    if *s == side && *place == root.place && dirs.iter().any(|dir| dir.starts_with(&root.dir)))
            })
        };
        (uses(self.ctx.staging(side)), uses(self.ctx.versions(side)))
    }

    /// Claims every dated folder the run keeps things in, bumping the attempt
    /// while one already exists. Returns whether the engine stopped.
    pub(super) async fn claim(&mut self, steps: &[Step]) -> bool {
        for side in [Side::Left, Side::Right] {
            let wanted = self.wanted(steps, side);
            if wanted == (false, false) {
                continue;
            }
            match self.claim_side(side, wanted).await {
                Some(Claim::Stopped) => return true,
                Some(Claim::Refused(root, result)) => self.blocked.push((root.place, side, root.dir, result)),
                Some(Claim::Made | Claim::Taken) => {}
                None => {
                    for root in roots_of(self.ctx.staging(side), self.ctx.versions(side), wanted) {
                        self.blocked
                            .push((root.place, side, root.dir, RunItemResult::CouldNotDo));
                    }
                }
            }
        }
        false
    }

    /// `None` when every attempt was taken.
    async fn claim_side(&mut self, side: Side, wanted: (bool, bool)) -> Option<Claim> {
        for attempt in 0..MAX_ATTEMPTS {
            if self.halted() {
                return Some(Claim::Stopped);
            }
            let Ok(next) = self.ctx.with_attempt(side, attempt) else {
                return None;
            };
            let roots = roots_of(next.staging(side), next.versions(side), wanted);
            let claim = self.claim_roots(side, &roots).await;
            if !matches!(claim, Claim::Taken) {
                self.ctx = next;
                return Some(claim);
            }
        }
        None
    }

    async fn claim_roots(&mut self, side: Side, roots: &[VersionsRoot]) -> Claim {
        for root in roots {
            if !self.safe_way(side, root).await {
                return Claim::Refused(root.clone(), RunItemResult::VersionsFolderUnsafe);
            }
            let batch = EngineBatch::ClaimFolder {
                place: root.place,
                side,
                dir: root.dir.clone(),
            };
            let outcome = self.engine.run(batch).await;
            match read(&outcome, &root.dir) {
                Some(ItemOutcome::Done) => self.claimed.push((side, root.clone())),
                Some(ItemOutcome::Skipped) => return Claim::Taken,
                Some(ItemOutcome::Failed) => return Claim::Refused(root.clone(), RunItemResult::CouldNotDo),
                None => return Claim::Stopped,
            }
        }
        Claim::Made
    }

    /// Every folder between the one the person chose and the dated folder is
    /// a real folder or not there yet: never a symlink, never a file.
    async fn safe_way(&self, side: Side, root: &VersionsRoot) -> bool {
        let Some(between) = root
            .dir
            .parent()
            .and_then(|parent| parent.strip_prefix(&root.trusted).ok())
        else {
            return false;
        };
        let mut path = root.trusted.clone();
        for part in between.components() {
            path.push(part);
            let now = self.engine.observe(side, root.place, &path, false).await;
            if now.is_some_and(|now| now.facts.kind != EntryKind::Folder) {
                return false;
            }
        }
        true
    }

    /// Moves the files the next copy replaces into the staging folder `to`
    /// on their own volume. Both the copy's source and the file are re-checked
    /// first, so a copy that can't run never costs the target its file.
    pub(super) async fn keep_aside(&mut self, side: Side, items: &[u32], to: &KeepDestination) -> bool {
        let KeepDestination::Folder { place, dir } = to else {
            self.settle_all(items, RunItemResult::CouldNotDo);
            return false;
        };
        if let Some(result) = self.blocked_at(*place, side, dir) {
            self.settle_all(items, result);
            return false;
        }
        let mut chosen = Vec::new();
        for &index in items {
            if self.halted() {
                break;
            }
            match self.keep_check(index, side, dir).await {
                Ok(source) => chosen.push((index, source)),
                Err(result) => self.settle(index, result),
            }
        }
        if self.halted() {
            return true;
        }
        let Some(outcome) = self.run_keep(side, to, &chosen).await else {
            return false;
        };
        for (index, source) in &chosen {
            match read(&outcome, &source.path) {
                Some(ItemOutcome::Done) => {
                    self.kept.insert(*index, dir.clone());
                }
                Some(other) => self.settle(*index, result_of(other)),
                // Stopped: if it went aside, it comes back with the leftovers.
                None => {
                    let old = self.item(*index).entry(side);
                    if let Some(old) = old
                        && self.went_to(side, old, to).await
                    {
                        self.kept.insert(*index, dir.clone());
                    }
                }
            }
        }
        outcome.stopped
    }

    async fn keep_check(&self, index: u32, side: Side, dir: &Path) -> Result<BoundSource, RunItemResult> {
        let item = self.item(index);
        let (Some(old), Some(source)) = (item.entry(side), item.entry(other(side))) else {
            return Err(RunItemResult::CouldNotDo);
        };
        if !self.unchanged(other(side), source).await || !self.unchanged(side, old).await {
            return Err(RunItemResult::ChangedSinceCompare);
        }
        if self
            .exists(side, Place::Target, &dir.join(file_name(&old.rel_path)))
            .await
        {
            return Err(RunItemResult::VersionsSpotTaken);
        }
        Ok(self.bound(side, old))
    }

    /// After a copy batch: a replacement that landed gets the policy (Trash,
    /// or the custom folder); one that didn't gets its old file back.
    pub(super) async fn finish_replacements(&mut self, items: &[u32]) {
        for &index in items {
            let Some(staged) = self.kept.get(&index).cloned() else {
                continue;
            };
            if self.results.get(&index) == Some(&RunItemResult::Done) {
                self.kept.remove(&index);
                self.apply_policy(index, &staged).await;
            } else {
                self.restore(&[index]).await;
            }
        }
    }

    /// Where a staged file ends up once its copy landed. Anything that can't
    /// happen leaves it in the staging folder, which keeps it too.
    async fn apply_policy(&mut self, index: u32, staged: &Path) {
        let item = self.item(index);
        let (Some(side), Some(old)) = (item.action.target(), item.action.target().and_then(|s| item.entry(s))) else {
            return;
        };
        let name = file_name(&old.rel_path);
        let source = BoundSource {
            path: staged.join(name),
            expected: old.facts,
        };
        let bytes = entry_bytes(old);
        let staging = self.ctx.staging(side).clone();
        let destination = keep_destination(
            &self.ctx.keep,
            self.ctx.caps(side),
            self.ctx.versions(side),
            &old.rel_path,
        );
        let to_trash = matches!(destination, KeepDestination::Trash);
        let moved = match destination {
            _ if self.halted() => false,
            KeepDestination::Trash => {
                self.run_one(EngineBatch::Trash {
                    side,
                    sources: vec![source],
                })
                .await
            }
            KeepDestination::Folder { place, dir } if place == Place::Target && dir == staged => false,
            KeepDestination::Folder { place, dir } => {
                let free =
                    self.blocked_at(place, side, &dir).is_none() && !self.exists(side, place, &dir.join(name)).await;
                let batch = EngineBatch::Move {
                    side,
                    sources: vec![source],
                    place,
                    dest_dir: dir,
                };
                if free && self.run_one(batch).await {
                    let versions = self.ctx.versions(side).clone();
                    self.note_kept(versions, bytes);
                    return;
                }
                false
            }
            KeepDestination::Refused(_) => false,
        };
        if !(moved && to_trash) {
            self.note_kept(staging, bytes);
        }
    }

    /// Runs a one-source batch to its end, even after a cancel (a policy move
    /// or a move back must not strand the file half way); whether it was done.
    /// After a stop, a source gone from its place (the staging folder, on the
    /// target's volume) counts as moved, so the report says where it is.
    async fn run_one(&mut self, batch: EngineBatch) -> bool {
        let source = match &batch {
            EngineBatch::Trash { side, sources }
            | EngineBatch::Move { side, sources, .. }
            | EngineBatch::MoveBack { side, sources, .. } => sources
                .first()
                .map(|source| (*side, source.path.clone(), source.expected)),
            EngineBatch::Copy { .. }
            | EngineBatch::MakeFolders { .. }
            | EngineBatch::ClaimFolder { .. }
            | EngineBatch::RemoveEmptyFolders { .. } => None,
        };
        let outcome = self.engine.run_to_end(batch).await;
        self.engine_stopped |= outcome.stopped;
        let Some((side, path, expected)) = source else {
            return false;
        };
        match read(&outcome, &path) {
            Some(done) => done == ItemOutcome::Done,
            // Only a stopped batch leaves a source unanswered (`read` turns any
            // other silence into `Failed`): not there as it was means moved.
            None if outcome.stopped => {
                let now = self.engine.observe(side, Place::Target, &path, false).await;
                now.is_none_or(|now| now.facts != expected)
            }
            None => false,
        }
    }

    /// Puts staged files back for these rows unless their copy landed. Runs
    /// after a cancel too: it undoes the run's own half step.
    pub(super) async fn restore(&mut self, items: &[u32]) {
        for &index in items {
            if self.results.get(&index) == Some(&RunItemResult::Done) {
                continue;
            }
            let Some(staged) = self.kept.remove(&index) else {
                continue;
            };
            if !self.move_back(index, &staged).await {
                self.settle(index, RunItemResult::KeptNotReplaced);
                let item = self.item(index);
                if let Some((side, old)) = item.action.target().and_then(|s| Some((s, item.entry(s)?))) {
                    let staging = self.ctx.staging(side).clone();
                    self.note_kept(staging, entry_bytes(old));
                }
            }
        }
    }

    async fn move_back(&mut self, index: u32, staged: &Path) -> bool {
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
        if self.exists(side, Place::Target, &original).await {
            return false;
        }
        let batch = EngineBatch::MoveBack {
            side,
            place: Place::Target,
            sources: vec![BoundSource {
                path: staged.join(file_name(&old.rel_path)),
                expected: old.facts,
            }],
            dest_dir: parent.to_path_buf(),
        };
        self.run_one(batch).await
    }
}
