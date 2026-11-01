//! Each `EngineBatch` as engine operations. Every landing is
//! `ConflictResolution::Skip` (an occupied spot is never overwritten), a claim
//! is exclusive, an empty-folder removal is one non-recursive `rmdir`, and
//! nothing here can delete permanently. `DETAILS.md` § "Engine adapter".

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::VolumeError;
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::{
    ConflictResolution, ExpectedSources, MutationError, OperationEventSink, SourceItemOutcome, VolumeCopyConfig,
    WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
};
use cmdr_sync::{BatchOutcome, BoundSource, EngineBatch, EntryFacts, EntryKind, ItemOutcome, Side};

use super::engine::{EngineAdapter, EngineSide, agrees};
use super::settle_sink::{SettleSink, Settled};

/// How often, after a forwarded cancel, the adapter looks whether a queued op
/// left the queue without settling (a queued op is dropped, never run).
const GONE_POLL: Duration = Duration::from_millis(100);
/// How long it then still waits for a settle that may be on its way.
const SETTLE_GRACE: Duration = Duration::from_millis(500);

/// What a transfer does with its sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Copy,
    Move,
}

/// Sources split by whether they still read as the snapshot did.
struct Bound {
    /// (the run's path, the path the engine sees).
    kept: Vec<(PathBuf, PathBuf)>,
    /// Changed or unreadable: never handed to the engine.
    refused: Vec<PathBuf>,
    expected: ExpectedSources,
}

fn other(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    }
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
                let outcome = self.claim(self.at(side, place), &dir).await;
                done(vec![(dir, outcome)])
            }
            EngineBatch::MakeFolders { place, side, dirs } => {
                let on = self.at(side, place);
                let mut items = Vec::with_capacity(dirs.len());
                for dir in dirs {
                    let outcome = if self.ensure_folder(on, &dir).await {
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
                let (source, target) = (self.side(from), self.side(other(from)));
                self.transfer(Verb::Copy, source, target, &sources, &dest_dir, cancellable)
                    .await
            }
            EngineBatch::Move {
                side,
                sources,
                place,
                dest_dir,
            } => {
                let (source, target) = (self.side(side), self.at(side, place));
                self.transfer(Verb::Move, source, target, &sources, &dest_dir, cancellable)
                    .await
            }
            EngineBatch::MoveBack {
                side,
                place,
                sources,
                dest_dir,
            } => {
                let (source, target) = (self.at(side, place), self.side(side));
                self.transfer(Verb::Move, source, target, &sources, &dest_dir, cancellable)
                    .await
            }
            EngineBatch::RemoveEmptyFolders { place, side, dirs } => {
                let on = self.at(side, place);
                let mut items = Vec::with_capacity(dirs.len());
                for dir in dirs {
                    let outcome = remove_empty(on, &dir).await;
                    items.push((dir, outcome));
                }
                done(items)
            }
            EngineBatch::Trash { side, sources } => self.trash(self.side(side), &sources, cancellable).await,
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

    async fn transfer(
        &self,
        verb: Verb,
        source: &EngineSide,
        target: &EngineSide,
        sources: &[BoundSource],
        dest_dir: &Path,
        cancellable: bool,
    ) -> BatchOutcome {
        let bound = self.bind(source, sources).await;
        if bound.kept.is_empty() {
            return skipped_only(&bound.refused);
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
        outcome_of(&bound, settled.as_ref(), verb == Verb::Move)
    }

    async fn trash(&self, on: &EngineSide, sources: &[BoundSource], cancellable: bool) -> BatchOutcome {
        // The Trash is the local Mac's; a volume without one never gets here
        // (the keep policy routes it to a versions folder), but is refused anyway.
        if !on.volume.has_trash() || on.volume.local_path().is_none() {
            let items = sources.iter().map(|s| (s.path.clone(), ItemOutcome::Failed)).collect();
            return done(items);
        }
        let bound = self.bind(on, sources).await;
        if bound.kept.is_empty() {
            return skipped_only(&bound.refused);
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
        outcome_of(&bound, settled.as_ref(), true)
    }

    /// Starts one operation with a `SettleSink` over the app's sink and waits
    /// for it to settle. A cancellable one gets the run's cancel forwarded.
    /// `None`: it refused to start.
    async fn settle<F, Fut>(&self, cancellable: bool, start: F) -> Option<Settled>
    where
        F: FnOnce(Arc<dyn OperationEventSink>) -> Fut,
        Fut: Future<Output = Result<WriteOperationStartResult, WriteOperationError>>,
    {
        let (sink, mut rx) = SettleSink::wrap((self.config.sinks)());
        let started = match start(Arc::clone(&sink) as Arc<dyn OperationEventSink>).await {
            Ok(started) => started,
            Err(_) => {
                // The refusal names paths; a log line names none (`LogPrivate`).
                log::warn!(target: "sync", "a sync batch didn't start");
                return None;
            }
        };
        self.operation_ids
            .lock_ignore_poison()
            .push(started.operation_id.clone());
        if !cancellable {
            return Some(rx.await.unwrap_or_else(|_| sink.heard()));
        }
        tokio::select! {
            settled = &mut rx => return Some(settled.unwrap_or_else(|_| sink.heard())),
            () = self.config.control.token().cancelled() => {}
        }
        let ops = self.config.operations;
        ops.cancel_operation(&OperationId::from(started.operation_id.clone()));
        loop {
            tokio::select! {
                settled = &mut rx => return Some(settled.unwrap_or_else(|_| sink.heard())),
                () = tokio::time::sleep(GONE_POLL) => {}
            }
            let known = ops
                .list_operations()
                .iter()
                .any(|op| op.operation_id == started.operation_id);
            if !known {
                let late = tokio::time::timeout(SETTLE_GRACE, &mut rx).await;
                let mut heard = late.ok().and_then(Result::ok).unwrap_or_else(|| sink.heard());
                heard.cancelled = true;
                return Some(heard);
            }
        }
    }

    /// Makes `dir` only if nothing is there: `Skipped` when something is.
    async fn claim(&self, on: &EngineSide, dir: &Path) -> ItemOutcome {
        if on.volume.get_metadata(dir).await.is_ok() {
            return ItemOutcome::Skipped;
        }
        let Some(parent) = dir.parent() else {
            return ItemOutcome::Failed;
        };
        if !self.ensure_folder(on, parent).await {
            return ItemOutcome::Failed;
        }
        match self.mkdir(on, dir).await {
            Ok(()) => ItemOutcome::Done,
            Err(MutationError::AlreadyExists { .. }) => ItemOutcome::Skipped,
            Err(_) => {
                // The refusal names paths; a log line names none (`LogPrivate`).
                log::warn!(target: "sync", "a versions folder couldn't be made");
                ItemOutcome::Failed
            }
        }
    }

    /// `dir` is a real folder afterwards, its missing parents made; a file or
    /// a symlink where a folder belongs refuses.
    async fn ensure_folder(&self, on: &EngineSide, dir: &Path) -> bool {
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
                Err(_) => {
                    // The refusal names paths; a log line names none (`LogPrivate`).
                    log::warn!(target: "sync", "a folder for the run couldn't be made: {}", LogPrivate::path(folder));
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
async fn remove_empty(on: &EngineSide, dir: &Path) -> ItemOutcome {
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
