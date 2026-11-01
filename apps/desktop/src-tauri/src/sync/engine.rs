//! `cmdr_sync::SyncEngine` over the app's write engine (`cmdr-operations`):
//! every copy, move, and trash is an ordinary engine operation (queue window,
//! progress, journal, undo), bound to the facts the compare saw. What each
//! batch does and why: `DETAILS.md` § "Engine adapter". The batches live in
//! `engine_ops.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{Volume, root_anchored};
use cmdr_operations::{LocalContent, OperationEventSink, Operations, RemoteContent, SourceFingerprint};
use cmdr_sync::{
    BatchOutcome, EngineBatch, EngineFuture, EntryFacts, EntryKind, Observed, Place, ScanOptions, Side, SyncControl,
    SyncEngine, subtree_now,
};

/// Makes the app's sink for one engine operation (`TauriEventSink` in the app).
pub(crate) type SinkFactory = Arc<dyn Fn() -> Arc<dyn OperationEventSink> + Send + Sync>;

/// One volume the adapter writes on.
#[derive(Clone)]
pub(crate) struct EngineSide {
    /// Its id in the volume registry (the engine resolves it there too).
    pub(crate) volume_id: String,
    /// The volume.
    pub(crate) volume: Arc<dyn Volume>,
    /// The sync root on it (the local disk's is unused).
    pub(crate) root: PathBuf,
}

impl EngineSide {
    /// The path the engine and the OS see: on a volume with a local path, the
    /// path anchored under it; elsewhere the volume path as is.
    pub(crate) fn engine_path(&self, path: &Path) -> PathBuf {
        match self.volume.local_path() {
            Some(local) => root_anchored(&local, path),
            None => path.to_path_buf(),
        }
    }
}

/// What the adapter runs with.
pub(crate) struct EngineConfig {
    /// The write engine.
    pub(crate) operations: &'static Operations,
    /// The left volume and root.
    pub(crate) left: EngineSide,
    /// The right volume and root.
    pub(crate) right: EngineSide,
    /// The local disk, for a custom versions folder (`Place::LocalDisk`).
    pub(crate) local: EngineSide,
    /// The compare's own scan options, so a folder re-check counts as the scan did.
    pub(crate) scan: ScanOptions,
    /// The run's cancel and pause.
    pub(crate) control: Arc<SyncControl>,
    /// The app's sink per operation.
    pub(crate) sinks: SinkFactory,
}

/// The adapter. Keeps the fingerprint each file had when it was last observed,
/// so the engine's own binding (`ExpectedSources`) holds it to that.
pub(crate) struct EngineAdapter {
    pub(super) config: EngineConfig,
    fingerprints: Mutex<HashMap<(String, PathBuf), SourceFingerprint>>,
    pub(super) operation_ids: Mutex<Vec<String>>,
}

impl EngineAdapter {
    pub(crate) fn new(config: EngineConfig) -> Self {
        Self {
            config,
            fingerprints: Mutex::new(HashMap::new()),
            operation_ids: Mutex::new(Vec::new()),
        }
    }

    /// The engine operations this adapter started, in order.
    pub(crate) fn operation_ids(&self) -> Vec<String> {
        self.operation_ids.lock_ignore_poison().clone()
    }

    pub(super) fn side(&self, side: Side) -> &EngineSide {
        match side {
            Side::Left => &self.config.left,
            Side::Right => &self.config.right,
        }
    }

    pub(super) fn at(&self, side: Side, place: Place) -> &EngineSide {
        match place {
            Place::Target => self.side(side),
            Place::LocalDisk => &self.config.local,
        }
    }

    /// Reads the engine's own fingerprint of `path` (lstat semantics: a link is
    /// the link).
    pub(super) async fn capture(on: &EngineSide, path: &Path) -> Option<SourceFingerprint> {
        if on.volume.local_path().is_some() {
            let os_path = on.engine_path(path);
            tokio::task::spawn_blocking(move || SourceFingerprint::capture_local(&os_path))
                .await
                .ok()
                .flatten()
        } else {
            SourceFingerprint::capture_remote(on.volume.as_ref(), path).await
        }
    }

    /// The fingerprint `observe` took of `path`, or a fresh one; taken out, so
    /// a later batch never reuses a stale one.
    pub(super) async fn fingerprint(&self, on: &EngineSide, path: &Path) -> Option<SourceFingerprint> {
        let key = (on.volume_id.clone(), path.to_path_buf());
        let cached = self.fingerprints.lock_ignore_poison().remove(&key);
        match cached {
            Some(found) => Some(found),
            None => Self::capture(on, path).await,
        }
    }

    /// The scan's options, with a never-cancelled control once the run is
    /// cancelled: the walks after a cancel (restore, tidy) are bookkeeping and
    /// need a true answer.
    fn walk_control(&self) -> Arc<SyncControl> {
        if self.config.control.is_cancelled() {
            SyncControl::new()
        } else {
            Arc::clone(&self.config.control)
        }
    }

    async fn observe_now(&self, side: Side, place: Place, path: &Path, with_subtree: bool) -> Option<Observed> {
        let on = self.at(side, place);
        let entry = on.volume.get_metadata(path).await.ok()?;
        let facts = EntryFacts::of(&entry);
        let key = (on.volume_id.clone(), path.to_path_buf());
        if facts.kind == EntryKind::Folder {
            self.fingerprints.lock_ignore_poison().remove(&key);
        } else {
            let found = Self::capture(on, path).await;
            let mut cache = self.fingerprints.lock_ignore_poison();
            match found {
                Some(found) => cache.insert(key, found),
                None => cache.remove(&key),
            };
        }
        let subtree = if with_subtree && facts.kind == EntryKind::Folder {
            // A walk that stops or fails leaves the subtree unknown, which never
            // matches a snapshot and never reads as empty.
            let rel = path
                .strip_prefix(&on.root)
                .map_or_else(|_| String::new(), |rel| rel.to_string_lossy().into_owned());
            let rel = if place == Place::Target { rel } else { String::new() };
            let control = self.walk_control();
            let walked = subtree_now(side, on.volume.as_ref(), path, &rel, &self.config.scan, &control).await;
            walked.ok()
        } else {
            None
        };
        Some(Observed { facts, subtree })
    }
}

/// Whether a captured fingerprint says the same as the snapshot's facts:
/// kind, and size and time where both are known (seconds, the listing's unit).
pub(super) fn agrees(found: &SourceFingerprint, expected: &EntryFacts) -> bool {
    let same = |known: Option<u64>, wanted: Option<u64>| match (known, wanted) {
        (Some(known), Some(wanted)) => known == wanted,
        _ => true,
    };
    match found {
        SourceFingerprint::Local {
            content: LocalContent::Directory,
            ..
        }
        | SourceFingerprint::Remote {
            content: RemoteContent::Directory,
            ..
        } => expected.kind == EntryKind::Folder,
        SourceFingerprint::Local {
            content: LocalContent::File { size, modified_nanos },
            ..
        } => {
            // A link's listing has no size; its own length is not a file size.
            let size = (expected.kind == EntryKind::File).then_some(*size);
            let secs = modified_nanos.and_then(|nanos| u64::try_from(nanos / 1_000_000_000).ok());
            expected.kind != EntryKind::Folder && same(size, expected.size) && same(secs, expected.mtime)
        }
        SourceFingerprint::Remote {
            content: RemoteContent::File { size, modified },
            ..
        } => {
            let secs = modified.and_then(|secs| u64::try_from(secs).ok());
            expected.kind != EntryKind::Folder && same(*size, expected.size) && same(secs, expected.mtime)
        }
    }
}

impl SyncEngine for EngineAdapter {
    fn observe<'a>(
        &'a self,
        side: Side,
        place: Place,
        path: &'a Path,
        with_subtree: bool,
    ) -> EngineFuture<'a, Option<Observed>> {
        Box::pin(self.observe_now(side, place, path, with_subtree))
    }

    fn run<'a>(&'a self, batch: EngineBatch) -> EngineFuture<'a, BatchOutcome> {
        Box::pin(self.run_batch(batch, true))
    }

    fn run_to_end<'a>(&'a self, batch: EngineBatch) -> EngineFuture<'a, BatchOutcome> {
        Box::pin(self.run_batch(batch, false))
    }
}
