//! What the write engine needs from the app around it, as named seams instead
//! of stashed `AppHandle`s and the Tauri runtime.
//!
//! - **The runtime** comes from the [`VolumeHost`]: one handle, so the engine
//!   and the backends it drives share one thread pool, and the two can't
//!   disagree.
//! - **Engine-wide facts** go through [`EngineEvents`] (`events.rs`).
//! - **One operation's events** go through [`OperationEventSink`]
//!   (`operation_sink.rs`), which each operation is handed at start.
//!
//! The app builds an [`EngineHost`] once (`operations_host::install`) and parks
//! it in a slot this module owns until the engine becomes a handle that carries
//! its seams itself.

mod events;
mod operation_sink;

use std::sync::{Arc, LazyLock, OnceLock};

use cmdr_fs::volume::host::VolumeHost;
use tokio::runtime::Handle;

use events::NoEngineEvents;
pub(crate) use events::{EngineEvents, InstantMutationFact, InstantOp, InstantOutcome, InstantTarget};
pub use operation_sink::OperationEventSink;

/// Everything the engine reaches the app through.
pub(crate) struct EngineHost {
    volume_host: VolumeHost,
    events: Arc<dyn EngineEvents>,
    /// What the archive-edit instant routes and the scan bridge still build a
    /// Tauri sink from.
    // shim: removed in M5b
    app: Option<tauri::AppHandle>,
}

impl EngineHost {
    /// A host over `volume_host` (whose runtime the engine spawns on) and
    /// `events`.
    pub(crate) fn new(volume_host: VolumeHost, events: Arc<dyn EngineEvents>) -> Self {
        Self {
            volume_host,
            events,
            app: None,
        }
    }

    /// Hands the remaining Tauri sink builders the app handle.
    // shim: removed in M5b
    pub(crate) fn with_app(mut self, app: tauri::AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// A host that answers nothing: no events, and `cmdr-fs`'s shared fallback
    /// runtime. ❌ Never a `Handle::current()` captured here: a runtime that is
    /// torn down later would orphan every operation admitted onto it.
    #[cfg_attr(
        test,
        expect(dead_code, reason = "a test binary falls back to the app's own answers")
    )]
    fn detached() -> Self {
        Self::new(VolumeHost::detached(), Arc::new(NoEngineEvents))
    }

    /// The runtime the engine spawns its background work on.
    pub(crate) fn runtime(&self) -> Handle {
        self.volume_host.runtime()
    }

    pub(crate) fn events(&self) -> &dyn EngineEvents {
        &*self.events
    }

    /// The startup-wired app handle. `None` before wiring and in every test, so
    /// a caller falls back to a plain refusal.
    // shim: removed in M5b
    pub(crate) fn app(&self) -> Option<&tauri::AppHandle> {
        self.app.as_ref()
    }
}

/// The host [`install`] parked.
// shim: removed in M6a
static INSTALLED: OnceLock<EngineHost> = OnceLock::new();

/// What [`installed`] answers before [`install`]. In the app nothing can start
/// an operation before setup, so this answers nothing, exactly as the unset
/// `AppHandle`s it replaces did. A test binary never installs, so it gets the
/// app's own answers minus the frontend channel (the real indexing gauge, the
/// real analytics client), the shape `volume_host::host()` hands an uninstalled
/// backend.
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
