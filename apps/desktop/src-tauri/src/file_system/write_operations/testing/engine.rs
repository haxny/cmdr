//! An engine of the test's own: [`TestOperations`], an [`Operations`] over one
//! fake per seam a test reads back, and [`TestOperationLog`], a journal over a
//! scratch database.
//!
//! Two engines share nothing, so a test on its own engine asserts on
//! engine-wide state (a manager counter, the busy set, `cancel_all`) without
//! seeing a sibling test's operations, under plain `cargo test` as under
//! nextest. Ids, lanes, and volume names only need to be distinct within the
//! one test.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_operation_log::OperationLog;
use cmdr_operation_log::capture::WriterJournal;
use cmdr_operation_log::store::operation_log_db_path;
use cmdr_operation_log::writer::OperationLogWriter;

use super::super::host::testing::{
    FakeVolumes, PaneListings, RecordingEngineEvents, RecordingOwnWrites, pane_order_host,
};
use super::super::state::WriteOperationState;
use super::super::{Operations, OperationsBuilder};
use cmdr_fs::testing::TestDir;

/// The progress interval of a state a test registers by hand: short, since a
/// synthetic operation never ticks anyway.
const TEST_PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// An [`Operations`] a test owns, over fakes it reads back: the engine-wide
/// facts it reported ([`Self::events`]), the volumes it knows
/// ([`Self::volumes`], which the test fills), and every path it announced
/// before writing ([`Self::own_writes`]). Every other seam answers nothing
/// unless the test replaces it ([`Self::with`]).
///
/// Derefs to the engine, so a test calls `engine.manager()`,
/// `engine.cancel_write_operation(…)`, and every other method on it directly.
pub(crate) struct TestOperations {
    engine: Operations,
    events: Arc<RecordingEngineEvents>,
    volumes: Arc<FakeVolumes>,
    own_writes: Arc<RecordingOwnWrites>,
}

impl TestOperations {
    /// An engine over fresh fakes that journals nothing.
    pub(crate) fn new() -> Self {
        Self::with(|builder| builder)
    }

    /// An engine journaling into `log`, for a test that reads back what its
    /// operations recorded.
    pub(crate) fn journaling_into(log: &TestOperationLog) -> Self {
        let log = log.log().clone();
        Self::with(|builder| builder.operation_log(log))
    }

    /// An engine whose host sees `panes`, for a test that seeds the
    /// fresh-listing oracle (`TestListing::insert(panes.panes(), …)`). On
    /// [`Self::new`]'s detached host every oracle question misses.
    pub(crate) fn seeing(panes: &Arc<PaneListings>) -> Self {
        let host = panes.host();
        Self::with(|builder| builder.volume_host(host))
    }

    /// An engine over fresh fakes, with `customize` replacing any other seam
    /// (a volume host, pacing, an index) on the builder. ❌ Don't replace the
    /// three fakes here: their accessors would then read a recorder the engine
    /// never calls.
    pub(crate) fn with(customize: impl FnOnce(OperationsBuilder) -> OperationsBuilder) -> Self {
        let events = Arc::new(RecordingEngineEvents::default());
        let volumes = Arc::new(FakeVolumes::default());
        let own_writes = Arc::new(RecordingOwnWrites::default());
        let builder = Operations::builder(volumes.clone(), events.clone()).own_writes(own_writes.clone());
        Self {
            engine: customize(builder).build(),
            events,
            volumes,
            own_writes,
        }
    }

    /// Every engine-wide fact this engine reported, in order.
    pub(crate) fn events(&self) -> &Arc<RecordingEngineEvents> {
        &self.events
    }

    /// The volumes this engine knows: register a test volume here before an
    /// operation names its id.
    pub(crate) fn volumes(&self) -> &Arc<FakeVolumes> {
        &self.volumes
    }

    /// Registers a local volume over `/` as `root`, the id a local operation
    /// names (and a `None` volume id means), so it takes the timed `Volume`
    /// path production takes.
    pub(crate) fn register_local_root(&self) {
        let root = cmdr_local::LocalPosixVolume::local_folder("Test root", "/", pane_order_host());
        self.volumes.register("root", Arc::new(root));
    }

    /// Every path this engine announced before writing it.
    #[expect(
        dead_code,
        reason = "the own-write pins read it back once they run on an engine of their own"
    )]
    pub(crate) fn own_writes(&self) -> &Arc<RecordingOwnWrites> {
        &self.own_writes
    }

    /// A fresh state on this engine, unregistered: what a start hands
    /// `spawn_managed`, or what a test drives an engine function with directly.
    pub(crate) fn new_state(&self) -> Arc<WriteOperationState> {
        Arc::new(self.new_operation_state(TEST_PROGRESS_INTERVAL))
    }

    /// A fresh state on this engine, registered under `operation_id`, as a
    /// running operation's is: what the by-id controls and status reads find.
    pub(crate) fn register(&self, operation_id: &str) -> RegisteredOperation {
        self.register_state(operation_id, self.new_state())
    }

    /// Registers a state the test built (with a seam armed, say) under
    /// `operation_id`. Build it on this engine (`new_operation_state`), so its
    /// scope reports to the same recorders.
    pub(crate) fn register_state(&self, operation_id: &str, state: Arc<WriteOperationState>) -> RegisteredOperation {
        self.registry().insert(operation_id.to_string(), Arc::clone(&state));
        RegisteredOperation {
            id: operation_id.to_string(),
            state,
        }
    }
}

impl std::ops::Deref for TestOperations {
    type Target = Operations;

    fn deref(&self) -> &Operations {
        &self.engine
    }
}

/// An operation's state, registered in a [`TestOperations`] registry. The
/// entry lives as long as the engine does; `forget` it from the registry to
/// settle it early.
pub(crate) struct RegisteredOperation {
    id: String,
    state: Arc<WriteOperationState>,
}

impl RegisteredOperation {
    /// The id it's registered under.
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// The registered state, for a test that reads `intent`, `backend_cancel`,
    /// or `pause_gate`, or arms a slot on it.
    pub(crate) fn state(&self) -> &Arc<WriteOperationState> {
        &self.state
    }
}

/// An operation log journaling into a database in a scratch folder, which a
/// test reads its engine's rows back from ([`Self::db`]). Hand it to
/// [`TestOperations::journaling_into`].
pub(crate) struct TestOperationLog {
    log: OperationLog,
    journal: Arc<WriterJournal>,
    db: PathBuf,
    _dir: TestDir,
}

impl TestOperationLog {
    /// A log over a fresh database in a scratch folder named after `label`.
    pub(crate) fn new(label: &str) -> Self {
        let dir = TestDir::new(label);
        let db = operation_log_db_path(&dir);
        let journal = Arc::new(WriterJournal::new(
            OperationLogWriter::spawn(&db).expect("a scratch operation-log writer"),
        ));
        let log = OperationLog::detached();
        log.set_journal(Some(journal.clone()));
        Self {
            log,
            journal,
            db,
            _dir: dir,
        }
    }

    /// The log itself, as an engine's builder takes it.
    pub(crate) fn log(&self) -> &OperationLog {
        &self.log
    }

    /// The writer behind the log, for a test that reverses what its engine
    /// journaled (`rollback_operation` takes it).
    pub(crate) fn writer(&self) -> &OperationLogWriter {
        self.journal.writer()
    }

    /// The database the log writes, for reading rows back.
    pub(crate) fn db(&self) -> &Path {
        &self.db
    }
}
