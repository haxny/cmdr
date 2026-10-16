//! What the write engine needs from the app around it, as named seams: the
//! engine names no `AppHandle` and no Tauri runtime.
//!
//! - **The runtime** comes from the [`VolumeHost`]: one handle, so the engine
//!   and the backends it drives share one thread pool, and the two can't
//!   disagree. The same host answers the fresh-listing oracle and the archive
//!   pane refresh (`listings()`), and every volume the engine mints gets it.
//! - **Which volume serves an id or a path** goes through [`OperationVolumes`]
//!   (`volumes.rs`), the host's registry as the engine asks it.
//! - **Engine-wide facts** go through [`EngineEvents`] (`events.rs`), error
//!   reports included, as typed [`OperationErrorReport`]s (`error_report.rs`).
//! - **One operation's events** go through [`OperationEventSink`]
//!   (`operation_sink.rs`), which each operation is handed at start; one scan
//!   preview's through [`ScanPreviewEventSink`] (`scan_preview_sink.rs`).
//! - **An operation the engine starts on its own behalf** (an instant op routed
//!   into a zip becomes a managed archive edit) gets its sink from the host's
//!   [`OperationSinkFactory`].
//!
//! The app builds an [`EngineHost`] once (`operations_host::install`) and parks
//! it in a slot this module owns until the engine becomes a handle that carries
//! its seams itself.

mod error_report;
mod events;
mod operation_sink;
mod scan_preview_sink;
mod volumes;

use std::sync::{Arc, LazyLock, OnceLock};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::ListingHost;
use tokio::runtime::Handle;

pub(crate) use error_report::{AsideAttempt, OperationErrorReport, TreeFailure};
use events::NoEngineEvents;
pub(crate) use events::{EngineEvents, InstantMutationFact, InstantOp, InstantOutcome, InstantTarget};
pub use operation_sink::OperationEventSink;
pub use scan_preview_sink::ScanPreviewEventSink;
use volumes::NoOperationVolumes;
pub(crate) use volumes::{OperationVolumes, UnregisteredVolume, VolumeAnswer};

/// Builds a fresh sink for an operation no caller handed one.
pub(crate) type OperationSinkFactory = Box<dyn Fn() -> Arc<dyn OperationEventSink> + Send + Sync>;

/// Everything the engine reaches the app through.
pub(crate) struct EngineHost {
    volume_host: VolumeHost,
    volumes: Arc<dyn OperationVolumes>,
    events: Arc<dyn EngineEvents>,
    operation_sinks: Option<OperationSinkFactory>,
}

impl EngineHost {
    /// A host over `volume_host` (whose runtime the engine spawns on),
    /// `volumes`, and `events`, with no sink factory.
    pub(crate) fn new(
        volume_host: VolumeHost,
        volumes: Arc<dyn OperationVolumes>,
        events: Arc<dyn EngineEvents>,
    ) -> Self {
        Self {
            volume_host,
            volumes,
            events,
            operation_sinks: None,
        }
    }

    /// Gives the engine a way to build a sink for an operation it starts on its
    /// own behalf.
    pub(crate) fn with_operation_sinks(mut self, factory: OperationSinkFactory) -> Self {
        self.operation_sinks = Some(factory);
        self
    }

    /// A host that answers nothing: no volumes, no events, and `cmdr-fs`'s shared fallback
    /// runtime. ❌ Never a `Handle::current()` captured here: a runtime that is
    /// torn down later would orphan every operation admitted onto it.
    #[cfg_attr(
        test,
        expect(dead_code, reason = "a test binary falls back to the app's own answers")
    )]
    fn detached() -> Self {
        Self::new(
            VolumeHost::detached(),
            Arc::new(NoOperationVolumes),
            Arc::new(NoEngineEvents),
        )
    }

    /// The runtime the engine spawns its background work on.
    pub(crate) fn runtime(&self) -> Handle {
        self.volume_host.runtime()
    }

    /// The volume host, for a volume the engine mints itself (a spool, a
    /// pull's scratch folder): it gets the same host the engine runs on.
    pub(crate) fn volume_host(&self) -> &VolumeHost {
        &self.volume_host
    }

    /// What the open panes show: the fresh-listing oracle, the archive pane
    /// refresh, and the pane patch after a local create.
    pub(crate) fn listings(&self) -> &dyn ListingHost {
        self.volume_host.listings()
    }

    /// Which volume serves an id or a path.
    pub(crate) fn volumes(&self) -> &dyn OperationVolumes {
        &*self.volumes
    }

    pub(crate) fn events(&self) -> &dyn EngineEvents {
        &*self.events
    }

    /// A fresh sink for an operation the engine starts on its own behalf.
    /// `None` when the host gave no factory (before `install`, and in every
    /// test), so a caller refuses plainly rather than run an op nobody hears.
    pub(crate) fn operation_sink(&self) -> Option<Arc<dyn OperationEventSink>> {
        self.operation_sinks.as_ref().map(|factory| factory())
    }
}

/// The host [`install`] parked.
// shim: removed in M6a
static INSTALLED: OnceLock<EngineHost> = OnceLock::new();

/// What [`installed`] answers before [`install`]. In the app nothing can start
/// an operation before setup, so this answers nothing. A test binary never
/// installs, so it gets the app's own answers minus the frontend channel (the
/// real indexing gauge, analytics client, and error reporter; no sink factory),
/// the shape `volume_host::host()` hands an uninstalled backend.
// shim: removed in M6a
static UNINSTALLED: LazyLock<EngineHost> = LazyLock::new(|| {
    #[cfg(test)]
    {
        crate::operations_host::without_frontend()
    }
    #[cfg(not(test))]
    {
        EngineHost::detached()
    }
});

/// Parks the app's host. Call once, in `setup()`, before anything can start an
/// operation.
// shim: removed in M6a
pub(crate) fn install(host: EngineHost) {
    if INSTALLED.set(host).is_err() {
        log::warn!(target: "op_manager", "the write engine's host was already installed; keeping the first one");
    }
}

/// The host the engine reaches the app through.
// shim: removed in M6a
pub(crate) fn installed() -> &'static EngineHost {
    INSTALLED.get().unwrap_or_else(|| &UNINSTALLED)
}
