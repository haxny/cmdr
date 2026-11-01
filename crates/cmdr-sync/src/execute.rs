//! Runs a confirmed plan through a write engine the app provides. Writes
//! nothing itself and has no way to delete: every replaced or removed item is
//! moved to the Trash or a versions folder. Why a trait, the re-checks, and
//! the restore rules: `DETAILS.md` § Run.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::runner::Runner;
use crate::{EntryFacts, Place, Plan, RunContext, Side, SubtreeFacts, SyncControl, VersionsRoot};

/// What every `SyncEngine` method returns.
pub type EngineFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a path reads like right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observed {
    /// The listing facts, as the scanner reads them.
    pub facts: EntryFacts,
    /// For a folder asked `with_subtree`: its contents counted exactly as the
    /// scan counted them (same options), else `None`.
    pub subtree: Option<SubtreeFacts>,
}

/// A source the engine re-checks itself before acting (`ExpectedSources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundSource {
    /// The absolute path.
    pub path: PathBuf,
    /// What the snapshot said about it.
    pub expected: EntryFacts,
}

/// One engine operation over a group of sources. ❌ No delete variant, on
/// purpose: a sync never destroys anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineBatch {
    /// Make `dir` (its parents as needed) only if it doesn't exist: `Done`
    /// when this call made it, `Skipped` when something was already there.
    /// The run's dated versions folders are claimed this way, so two runs
    /// never share one.
    ClaimFolder {
        /// Which volume.
        place: Place,
        /// The side it belongs to.
        side: Side,
        /// The folder.
        dir: PathBuf,
    },
    /// Make these folders with any missing parents; one that exists is fine.
    MakeFolders {
        /// Which volume.
        place: Place,
        /// The side they belong to.
        side: Side,
        /// The folders.
        dirs: Vec<PathBuf>,
    },
    /// Copy into `dest_dir` on the other side, landing through temp+rename with
    /// `ConflictResolution::Skip`: anything already at a landing spot stays.
    Copy {
        /// The side copied from.
        from: Side,
        /// The sources.
        sources: Vec<BoundSource>,
        /// The folder they land in.
        dest_dir: PathBuf,
    },
    /// Move `side`'s sources into `dest_dir` on `place` (keeping them), with
    /// `ConflictResolution::Skip`: a source whose landing spot is taken is
    /// `Skipped` and stays where it is, never overwriting what is there.
    Move {
        /// The side the sources are on.
        side: Side,
        /// The sources.
        sources: Vec<BoundSource>,
        /// The volume `dest_dir` is on.
        place: Place,
        /// The folder they go into.
        dest_dir: PathBuf,
    },
    /// Move kept items from `place` back into `dest_dir` on `side`, with
    /// `ConflictResolution::Skip`. Undoes a keep whose copy didn't land.
    MoveBack {
        /// The side they go back to.
        side: Side,
        /// The volume the kept items are on.
        place: Place,
        /// The kept items.
        sources: Vec<BoundSource>,
        /// Their original folder.
        dest_dir: PathBuf,
    },
    /// Move `side`'s sources to its Trash.
    Trash {
        /// The side the sources are on.
        side: Side,
        /// The sources.
        sources: Vec<BoundSource>,
    },
}

/// What the engine did with one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemOutcome {
    /// Done.
    Done,
    /// Left alone: it changed since it was bound, or something sat where it
    /// would land.
    Skipped,
    /// Tried and couldn't.
    Failed,
}

/// What the engine did with a batch, per source path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchOutcome {
    /// Sources it got to.
    pub items: Vec<(PathBuf, ItemOutcome)>,
    /// It stopped before the end (cancelled from its own UI, say).
    pub stopped: bool,
}

/// The write engine, as the executor sees it. The app implements it over
/// `cmdr_operations::Operations`.
pub trait SyncEngine: Send + Sync {
    /// What is at `path` now, on `side`'s volume (`Place::Target`) or the
    /// local disk; `None` when nothing is. A symlink reads as `Symlink`, never
    /// followed.
    fn observe<'a>(
        &'a self,
        side: Side,
        place: Place,
        path: &'a Path,
        with_subtree: bool,
    ) -> EngineFuture<'a, Option<Observed>>;
    /// Runs one batch to its end.
    fn run<'a>(&'a self, batch: EngineBatch) -> EngineFuture<'a, BatchOutcome>;
}

/// How one row of the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RunItemResult {
    /// Done; anything it replaced or removed is kept.
    Done,
    /// Something it reads or replaces changed since compare: left alone.
    ChangedSinceCompare,
    /// Trash only, and the volume has none: left alone.
    NoTrash,
    /// The engine couldn't, or the keep folder couldn't be made: nothing was
    /// lost.
    CouldNotDo,
    /// A removal held because a copy into the same folder didn't land (for any
    /// reason): in Mirror a rename is a copy plus a removal.
    HeldAfterFailedCopy,
    /// Something already sits where the item would be kept: left alone.
    VersionsSpotTaken,
    /// The way to the versions folder runs through a symlink or a file: left
    /// alone.
    VersionsFolderUnsafe,
    /// The old file is kept (Trash or versions folder) but the new copy didn't
    /// land and the old one couldn't go back: the place is empty.
    KeptNotReplaced,
    /// The run stopped first.
    NotStarted,
}

/// Running totals, after each step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunProgress {
    /// Steps finished.
    pub steps_done: u32,
    /// Steps in the run.
    pub steps_total: u32,
    /// Rows with a result.
    pub items_done: u64,
    /// Rows in the run.
    pub items_total: u64,
    /// Their bytes.
    pub bytes_done: u64,
    /// All rows' bytes.
    pub bytes_total: u64,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    /// Every acting row of the run, by index.
    pub results: Vec<(u32, RunItemResult)>,
    /// Versions folders that received something.
    pub versions_used: Vec<VersionsRoot>,
    /// Bytes put in versions folders.
    pub kept_bytes: u64,
    /// Target folders the run made for copies that then all didn't land; they
    /// stay (the run never removes anything) and are empty.
    pub empty_folders: Vec<(Side, PathBuf)>,
    /// Cancelled, or the engine stopped.
    pub stopped: bool,
}

/// Runs the rows of `plan` listed in `run` (from `gate`). Pause and Cancel
/// land between batches; a cancelled run puts back what it set aside for
/// copies that never ran.
pub async fn execute(
    plan: &Plan,
    run: &[u32],
    ctx: &RunContext,
    engine: &dyn SyncEngine,
    control: &Arc<SyncControl>,
    on_progress: &(dyn Fn(RunProgress) + Sync),
) -> RunReport {
    Runner::new(plan, ctx, engine, control, on_progress).run(run).await
}
