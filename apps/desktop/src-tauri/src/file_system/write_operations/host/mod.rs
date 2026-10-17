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
//! - **"Cmdr is about to write here"** goes through [`OwnWrites`]
//!   (`own_writes.rs`), before every syscall that lands a path.
//! - **What the drive index knows** goes through [`OperationIndex`]
//!   (`index.rs`).
//! - **Deliberate slowness for automated runs** goes through [`Pacing`]
//!   (`pacing.rs`).
//! - **The collator the panes sort with** comes from [`NameOrder`]
//!   (`name_order.rs`).
//!
//! The volumes and the events are required; every other seam defaults to an
//! answer that does nothing, which the host replaces with `with_*`.
//!
//! The host hands these to `Operations::builder`; the engine keeps them as one
//! [`EngineHost`], shared by every operation's scope.

mod error_report;
mod events;
mod index;
mod name_order;
mod operation_sink;
mod own_writes;
mod pacing;
mod scan_preview_sink;
// Fakes for the seams a test reads back.
#[cfg(test)]
pub(crate) mod testing;
mod volumes;

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::ListingHost;
use tokio::runtime::Handle;

pub(crate) use error_report::{AsideAttempt, OperationErrorReport, TreeFailure};
#[cfg(any(test, feature = "transfer-bench"))]
use events::NoEngineEvents;
pub(crate) use events::{EngineEvents, InstantMutationFact, InstantOp, InstantOutcome, InstantTarget};
use index::NoOperationIndex;
pub(crate) use index::{BufferedLeaves, ExpectedTotals, Leaf, OperationIndex};
use name_order::EnglishNameOrder;
pub(crate) use name_order::NameOrder;
pub use operation_sink::OperationEventSink;
use own_writes::NoOwnWrites;
pub(crate) use own_writes::OwnWrites;
use pacing::NoPacing;
pub(crate) use pacing::Pacing;
pub use scan_preview_sink::ScanPreviewEventSink;
#[cfg(any(test, feature = "transfer-bench"))]
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
    own_writes: Arc<dyn OwnWrites>,
    index: Arc<dyn OperationIndex>,
    pacing: Arc<dyn Pacing>,
    name_order: Arc<dyn NameOrder>,
}

impl EngineHost {
    /// A host over `volume_host` (whose runtime the engine spawns on),
    /// `volumes`, and `events`, with no sink factory and every other seam
    /// answering nothing.
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
            own_writes: Arc::new(NoOwnWrites),
            index: Arc::new(NoOperationIndex),
            pacing: Arc::new(NoPacing),
            name_order: Arc::new(EnglishNameOrder::new()),
        }
    }

    /// Gives the engine a way to build a sink for an operation it starts on its
    /// own behalf.
    pub(crate) fn with_operation_sinks(mut self, factory: OperationSinkFactory) -> Self {
        self.operation_sinks = Some(factory);
        self
    }

    /// Tells `own_writes` about every path the engine is about to write.
    pub(crate) fn with_own_writes(mut self, own_writes: Arc<dyn OwnWrites>) -> Self {
        self.own_writes = own_writes;
        self
    }

    /// Answers the engine's drive-index questions from `index`.
    pub(crate) fn with_index(mut self, index: Arc<dyn OperationIndex>) -> Self {
        self.index = index;
        self
    }

    /// Paces the engine where it deliberately waits, as `pacing` says.
    pub(crate) fn with_pacing(mut self, pacing: Arc<dyn Pacing>) -> Self {
        self.pacing = pacing;
        self
    }

    /// Orders names the way `name_order` says the panes do.
    pub(crate) fn with_name_order(mut self, name_order: Arc<dyn NameOrder>) -> Self {
        self.name_order = name_order;
        self
    }

    /// A host that answers nothing: no volumes, no events, and `cmdr-fs`'s shared fallback
    /// runtime. ❌ Never a `Handle::current()` captured here: a runtime that is
    /// torn down later would orphan every operation admitted onto it.
    #[cfg(any(test, feature = "transfer-bench"))]
    pub(crate) fn detached() -> Self {
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

    /// The engine-wide events seam itself, for a part of the engine that keeps
    /// its own handle on it (the status cache).
    pub(crate) fn events_arc(&self) -> &Arc<dyn EngineEvents> {
        &self.events
    }

    /// Where the engine announces a path before it writes there.
    pub(crate) fn own_writes(&self) -> &dyn OwnWrites {
        &*self.own_writes
    }

    /// What the drive index knows.
    pub(crate) fn index(&self) -> &dyn OperationIndex {
        &*self.index
    }

    /// How long to wait where the engine deliberately waits.
    pub(crate) fn pacing(&self) -> &dyn Pacing {
        &*self.pacing
    }

    /// The collator the panes sort names with.
    pub(crate) fn name_order(&self) -> &dyn NameOrder {
        &*self.name_order
    }

    /// A fresh sink for an operation the engine starts on its own behalf.
    /// `None` when the host gave no factory (before `install`, and in every
    /// test), so a caller refuses plainly rather than run an op nobody hears.
    pub(crate) fn operation_sink(&self) -> Option<Arc<dyn OperationEventSink>> {
        self.operation_sinks.as_ref().map(|factory| factory())
    }
}
