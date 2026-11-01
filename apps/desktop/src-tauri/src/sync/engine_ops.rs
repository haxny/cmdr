//! Each `EngineBatch` as engine operations. Every landing is
//! `ConflictResolution::Skip` (an occupied spot is never overwritten), a claim
//! is exclusive, an empty-folder removal is one non-recursive `rmdir`, and
//! nothing here can delete permanently. `DETAILS.md` § "Engine adapter".

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::VolumeError;
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::{
    ConflictResolution, ExpectedSources, MutationError, OperationEventSink, SourceItemOutcome, VolumeCopyConfig,
    WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
};
use cmdr_sync::{BatchOutcome, BoundSource, EngineBatch, EntryFacts, EntryKind, ItemOutcome, Place, Side};

use super::engine::{EngineAdapter, EngineSide, agrees};
use super::settle_sink::{SettleSink, Settled};

/// How often, while waiting, the adapter looks whether its op left the queue
/// without settling: a queued op cancelled (by the run or from the queue
/// window) is dropped, never run, and its sink never hears an end.
const GONE_POLL: Duration = Duration::from_millis(100);
/// How long it then still waits for a settle that may be on its way.
const SETTLE_GRACE: Duration = Duration::from_millis(500);

/// What a transfer does with its sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Copy,
    Move,
}

/// Where a transfer lands, and the root below which no symlink may lead there.
struct Landing<'a> {
    target: &'a EngineSide,
    base: Option<&'a Path>,
    dest_dir: &'a Path,
}

/// Where a transfer's sources sit. A move takes them away, so no symlink below
/// `base` may lead there either (`None`: a copy, which only reads them).
struct Origin<'a> {
    on: &'a EngineSide,
    base: Option<&'a Path>,
}

/// Sources split by whether they still read as the snapshot did.
struct Bound {
    /// (the run's path, the path the engine sees).
    kept: Vec<(PathBuf, PathBuf)>,
    /// Changed or unreadable: never handed to the engine.
    refused: Vec<PathBuf>,
    expected: ExpectedSources,
}

fn stopped_empty() -> BatchOutcome {
    BatchOutcome {
        items: Vec::new(),
        stopped: true,
    }
}

impl EngineAdapter {
    pub(super) async fn run_batch(&self, batch: EngineBatch, cancellable: bool) -> BatchOutcome {
        // A cancel that came first lands before the engine call.
        if cancellable && self.config.control.is_cancelled() {
            return stopped_empty();
        }
        match batch {
            EngineBatch::ClaimFolder { place, side, dir } => {
                let base = self.base(side, place);
                let outcome = self.claim(self.at(side, place), base.as_deref(), &dir).await;
                done(vec![(dir, outcome)])
            }
            EngineBatch::MakeFolders { place, side, dirs } => {
                let (on, base) = (self.at(side, place), self.base(side, place));
                let mut items = Vec::with_capacity(dirs.len());
                for dir in dirs {
                    let outcome = if self.ensure_folder(on, base.as_deref(), &dir).await {
                        ItemOutcome::Done
                    } else {
                        ItemOutcome::Failed
                    };
                    items.push((dir, outcome));
                }
                done(items)
            }
            EngineBatch::Copy {
                from,
                sources,
                dest_dir,
            } => {
                let (source, target) = (self.side(from), self.side(from.other()));
                let base = self.base(from.other(), Place::Target);
                let landing = Landing {
                    target,
                    base: base.as_deref(),
                    dest_dir: &dest_dir,
                };
                let origin = Origin { on: source, base: None };
                self.transfer(Verb::Copy, &origin, &landing, &sources, cancellable)
                    .await
            }
            EngineBatch::Move {
                side,
                sources,
                place,
                dest_dir,
            } => {
                let (source, target) = (self.side(side), self.at(side, place));
                let (base, source_base) = (self.base(side, place), self.base(side, Place::Target));
                let landing = Landing {
                    target,
                    base: base.as_deref(),
                    dest_dir: &dest_dir,
                };
                let origin = Origin {
                    on: source,
                    base: source_base.as_deref(),
                };
                self.transfer(Verb::Move, &origin, &landing, &sources, cancellable)
                    .await
            }
            EngineBatch::MoveBack {
                side,
                place,
                sources,
                dest_dir,
            } => {
                let (source, target) = (self.at(side, place), self.side(side));
                let (base, source_base) = (self.base(side, Place::Target), self.base(side, place));
                let landing = Landing {
                    target,
                    base: base.as_deref(),
                    dest_dir: &dest_dir,
                };
                let origin = Origin {
                    on: source,
                    base: source_base.as_deref(),
                };
                self.transfer(Verb::Move, &origin, &landing, &sources, cancellable)
                    .await
            }
            EngineBatch::RemoveEmptyFolders { place, side, dirs } => {
                let (on, base) = (self.at(side, place), self.base(side, place));
                let mut items = Vec::with_capacity(dirs.len());
                for dir in dirs {
                    let outcome = remove_empty(on, base.as_deref(), &dir).await;
                    items.push((dir, outcome));
                }
                done(items)
            }
            EngineBatch::Trash { side, sources } => {
                let base = self.base(side, Place::Target);
                let origin = Origin {
                    on: self.side(side),
                    base: base.as_deref(),
                };
                self.trash(&origin, &sources, cancellable).await
            }
        }
    }

    /// Holds each source to the fingerprint `observe` took (or one taken now),
    /// which must say what the snapshot said.
    async fn bind(&self, on: &EngineSide, sources: &[BoundSource]) -> Bound {
        let mut kept = Vec::new();
        let mut refused = Vec::new();
        let mut expected = Vec::new();
        for source in sources {
            match self.fingerprint(on, &source.path).await {
                Some(found) if agrees(&found, &source.expected) => {
                    let engine_path = on.engine_path(&source.path);
                    expected.push((engine_path.clone(), found));
                    kept.push((source.path.clone(), engine_path));
                }
                _ => refused.push(source.path.clone()),
            }
        }
        Bound {
            kept,
            refused,
            expected: ExpectedSources::new(expected),
        }
    }

    /// The folder under which no symlink may lead a write: the side's root on
    /// its own volume. The local disk's custom folder is checked by the run
    /// from the folder the person picked (`safe_way`).
    fn base(&self, side: Side, place: Place) -> Option<PathBuf> {
        (place == Place::Target).then(|| self.side(side).root.clone())
    }

    async fn transfer(
        &self,
        verb: Verb,
        origin: &Origin<'_>,
        landing: &Landing<'_>,
        sources: &[BoundSource],
        cancellable: bool,
    ) -> BatchOutcome {
        let (target, dest_dir) = (landing.target, landing.dest_dir);
        if !no_link_below(target, landing.base, dest_dir).await {
            let items = sources.iter().map(|s| (s.path.clone(), ItemOutcome::Failed)).collect();
            return done(items);
        }
        let source = origin.on;
        let (sources, linked) = unlinked(origin, sources).await;
        let bound = self.bind(source, &sources).await;
        if bound.kept.is_empty() {
            return with_failed(skipped_only(&bound.refused), linked);
        }
        let ops = self.config.operations;
        let config = VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Skip,
            ..VolumeCopyConfig::default()
        };
        let paths: Vec<PathBuf> = bound.kept.iter().map(|(_, engine)| engine.clone()).collect();
        let dest = target.engine_path(dest_dir).to_string_lossy().into_owned();
        let (from_id, to_id) = (source.volume_id.clone(), target.volume_id.clone());
        let expected = Some(bound.expected.clone());
        let settled = self
            .settle(cancellable, |sink| async move {
                match verb {
                    Verb::Copy => {
                        ops.start_volume_copy(sink, from_id, paths, to_id, dest, config, Initiator::User, expected)
                            .await
                    }
                    Verb::Move => {
                        ops.start_volume_move(sink, from_id, paths, to_id, dest, config, Initiator::User, expected)
                            .await
                    }
                }
            })
            .await;
        with_failed(outcome_of(&bound, settled.as_ref(), verb == Verb::Move), linked)
    }

    async fn trash(&self, origin: &Origin<'_>, sources: &[BoundSource], cancellable: bool) -> BatchOutcome {
        let on = origin.on;
        // The Trash is the local Mac's; a volume without one never gets here
        // (the keep policy routes it to a versions folder), but is refused anyway.
        if !on.volume.has_trash() || on.volume.local_path().is_none() {
            let items = sources.iter().map(|s| (s.path.clone(), ItemOutcome::Failed)).collect();
            return done(items);
        }
        let (sources, linked) = unlinked(origin, sources).await;
        let bound = self.bind(on, &sources).await;
        if bound.kept.is_empty() {
            return with_failed(skipped_only(&bound.refused), linked);
        }
        let ops = self.config.operations;
        let paths: Vec<PathBuf> = bound.kept.iter().map(|(_, engine)| engine.clone()).collect();
        let expected = Some(bound.expected.clone());
        let settled = self
            .settle(cancellable, |sink| async move {
                let config = WriteOperationConfig::default();
                ops.trash_files_start(sink, paths, None, config, Initiator::User, expected)
                    .await
            })
            .await;
        with_failed(outcome_of(&bound, settled.as_ref(), true), linked)
    }

    /// Starts one operation with a `SettleSink` over the app's sink and waits
    /// for it to settle. A cancellable one gets the run's cancel forwarded.
    /// Either way, an op that leaves the queue without settling (cancelled
    /// while queued, here or from the queue window) ends the wait as
    /// cancelled. `None`: it refused to start.
    async fn settle<F, Fut>(&self, cancellable: bool, start: F) -> Option<Settled>
    where
        F: FnOnce(Arc<dyn OperationEventSink>) -> Fut,
        Fut: Future<Output = Result<WriteOperationStartResult, WriteOperationError>>,
    {
        let (sink, mut rx) = SettleSink::wrap((self.config.sinks)());
        let started = match start(Arc::clone(&sink) as Arc<dyn OperationEventSink>).await {
            Ok(started) => started,
            Err(refusal) => {
                // The refusal names paths; a log line names only its kind.
                log::warn!(target: "sync", "a sync batch didn't start: {}", kind_of(&refusal));
                return None;
            }
        };
        let id = started.operation_id;
        self.operation_ids.lock_ignore_poison().push(id.clone());
        let ops = self.config.operations;
        let token = self.config.control.token();
        let mut forwarded = !cancellable;
        loop {
            tokio::select! {
                settled = &mut rx => return Some(settled.unwrap_or_else(|_| sink.heard())),
                () = token.cancelled(), if !forwarded => {
                    ops.cancel_operation(&OperationId::from(id.clone()));
                    forwarded = true;
                }
                () = tokio::time::sleep(GONE_POLL) => {}
            }
            if !ops.list_operations().iter().any(|op| op.operation_id == id) {
                // A settle may still be on its way (an op that just finished
                // leaves the list first); it says for itself how it ended.
                // Silence means it was dropped unrun: cancelled.
                return Some(match tokio::time::timeout(SETTLE_GRACE, &mut rx).await {
                    Ok(Ok(settled)) => settled,
                    Ok(Err(_)) | Err(_) => Settled {
                        cancelled: true,
                        ..sink.heard()
                    },
                });
            }
        }
    }

    /// Makes `dir` only if nothing is there: `Skipped` when something is.
    async fn claim(&self, on: &EngineSide, base: Option<&Path>, dir: &Path) -> ItemOutcome {
        if on.volume.get_metadata(dir).await.is_ok() {
            return ItemOutcome::Skipped;
        }
        let Some(parent) = dir.parent() else {
            return ItemOutcome::Failed;
        };
        if !self.ensure_folder(on, base, parent).await {
            return ItemOutcome::Failed;
        }
        match self.mkdir(on, dir).await {
            Ok(()) => ItemOutcome::Done,
            Err(MutationError::AlreadyExists { .. }) => ItemOutcome::Skipped,
            Err(refusal) => {
                // The refusal names paths; a log line names only its kind.
                log::warn!(target: "sync", "a versions folder couldn't be made: {}", kind_of(&refusal));
                ItemOutcome::Failed
            }
        }
    }

    /// `dir` is a real folder afterwards, its missing parents made; a file or
    /// a symlink where a folder belongs refuses.
    async fn ensure_folder(&self, on: &EngineSide, base: Option<&Path>, dir: &Path) -> bool {
        if !no_link_below(on, base, dir).await {
            return false;
        }
        let mut missing = Vec::new();
        for ancestor in dir.ancestors() {
            if ancestor.file_name().is_none() {
                break;
            }
            match on.volume.get_metadata(ancestor).await {
                Ok(entry) if EntryFacts::of(&entry).kind == EntryKind::Folder => break,
                Ok(_) => return false,
                Err(_) => missing.push(ancestor.to_path_buf()),
            }
        }
        for folder in missing.iter().rev() {
            match self.mkdir(on, folder).await {
                Ok(()) => {}
                Err(MutationError::AlreadyExists { .. }) if is_folder(on, folder).await => {}
                Err(refusal) => {
                    // The refusal names paths; a log line names only its kind.
                    log::warn!(target: "sync", "a folder for the run couldn't be made: {}", kind_of(&refusal));
                    return false;
                }
            }
        }
        true
    }

    async fn mkdir(&self, on: &EngineSide, dir: &Path) -> Result<(), MutationError> {
        let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
            return Err(MutationError::NameEmpty);
        };
        let parent = on.engine_path(parent).to_string_lossy().into_owned();
        let name = name.to_string_lossy().into_owned();
        self.config
            .operations
            .create_directory_managed(Some(on.volume_id.clone()), parent, name, Initiator::User)
            .await
            .map(|_| ())
    }
}

async fn is_folder(on: &EngineSide, path: &Path) -> bool {
    on.volume
        .get_metadata(path)
        .await
        .is_ok_and(|entry| EntryFacts::of(&entry).kind == EntryKind::Folder)
}

/// One non-recursive `rmdir`: a link is not a folder, and anything inside
/// (hidden files too) refuses.
async fn remove_empty(on: &EngineSide, base: Option<&Path>, dir: &Path) -> ItemOutcome {
    if !no_link_below(on, base, dir.parent().unwrap_or(dir)).await {
        return ItemOutcome::Failed;
    }
    match on.volume.get_metadata(dir).await {
        Ok(entry) if EntryFacts::of(&entry).kind == EntryKind::Folder => {}
        Ok(_) => return ItemOutcome::Failed,
        Err(VolumeError::NotFound(_)) => return ItemOutcome::Skipped,
        Err(_) => return ItemOutcome::Failed,
    }
    if on.volume.local_path().is_some() {
        let os_path = on.engine_path(dir);
        let removed = tokio::task::spawn_blocking(move || std::fs::remove_dir(os_path)).await;
        return match removed {
            Ok(Ok(())) => ItemOutcome::Done,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => ItemOutcome::Skipped,
            Ok(Err(_)) | Err(_) => ItemOutcome::Failed,
        };
    }
    match on.volume.delete_directory(dir, None).await {
        Ok(()) => ItemOutcome::Done,
        Err(VolumeError::NotFound(_)) => ItemOutcome::Skipped,
        Err(_) => ItemOutcome::Failed,
    }
}

/// Whether every folder from just below `base` down to `path` that exists is
/// not a symlink (lstat), so a write there can't be led out of the root. A
/// path outside `base`, or a folder that won't answer, is refused; `None`
/// checks nothing.
pub(super) async fn no_link_below(on: &EngineSide, base: Option<&Path>, path: &Path) -> bool {
    let Some(base) = base else {
        return true;
    };
    let Ok(rest) = path.strip_prefix(base) else {
        return false;
    };
    let mut at = base.to_path_buf();
    for part in rest.components() {
        at.push(part);
        match on.volume.get_metadata(&at).await {
            Ok(entry) if EntryFacts::of(&entry).kind == EntryKind::Symlink => return false,
            Ok(_) => {}
            // Nothing there yet: nothing further down exists either.
            Err(VolumeError::NotFound(_)) => return true,
            // A folder that won't answer can't be shown not to be a link.
            Err(_) => return false,
        }
    }
    true
}

/// Splits `sources` into those reached through real folders only (lstat, from
/// just below the origin's base down to each one's parent) and the paths of
/// those a symlink leads to, which a move or trash must never take: a folder
/// swapped for a link since the observe would carry the move outside the root.
async fn unlinked(origin: &Origin<'_>, sources: &[BoundSource]) -> (Vec<BoundSource>, Vec<PathBuf>) {
    let mut through = Vec::with_capacity(sources.len());
    let mut linked = Vec::new();
    for source in sources {
        let parent = source.path.parent().unwrap_or(&source.path);
        if origin.base.is_none() || no_link_below(origin.on, origin.base, parent).await {
            through.push(source.clone());
        } else {
            linked.push(source.path.clone());
        }
    }
    (through, linked)
}

/// `outcome` with `linked` added as `Failed`.
fn with_failed(outcome: BatchOutcome, linked: Vec<PathBuf>) -> BatchOutcome {
    let mut items = outcome.items;
    items.extend(linked.into_iter().map(|path| (path, ItemOutcome::Failed)));
    BatchOutcome { items, ..outcome }
}

/// A refusal's typed kind (its serde tag), for a log line without its paths.
fn kind_of(refusal: &impl serde::Serialize) -> String {
    serde_json::to_value(refusal)
        .ok()
        .and_then(|value| value.get("type").and_then(|kind| kind.as_str().map(str::to_owned)))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn done(items: Vec<(PathBuf, ItemOutcome)>) -> BatchOutcome {
    BatchOutcome { items, stopped: false }
}

fn skipped_only(refused: &[PathBuf]) -> BatchOutcome {
    done(
        refused
            .iter()
            .map(|path| (path.clone(), ItemOutcome::Skipped))
            .collect(),
    )
}

/// The engine's words mapped back onto the run's paths. A move or trash is
/// `Done` only when the source left its place. A source the engine never
/// answered for is left out: the runner decides what that means.
fn outcome_of(bound: &Bound, settled: Option<&Settled>, must_leave: bool) -> BatchOutcome {
    let mut items: Vec<(PathBuf, ItemOutcome)> = bound
        .refused
        .iter()
        .map(|path| (path.clone(), ItemOutcome::Skipped))
        .collect();
    let Some(settled) = settled else {
        items.extend(bound.kept.iter().map(|(path, _)| (path.clone(), ItemOutcome::Failed)));
        return done(items);
    };
    for (path, engine_path) in &bound.kept {
        let Some(end) = settled.end_of(engine_path) else {
            continue;
        };
        let outcome = match end.outcome {
            SourceItemOutcome::Done if must_leave && !end.removed => ItemOutcome::Failed,
            SourceItemOutcome::Done => ItemOutcome::Done,
            SourceItemOutcome::Skipped => ItemOutcome::Skipped,
            SourceItemOutcome::Failed => ItemOutcome::Failed,
        };
        items.push((path.clone(), outcome));
    }
    BatchOutcome {
        items,
        stopped: settled.cancelled,
    }
}
