//! This app, acting as the write engine's host.
//!
//! The mirror of `file_system::write_operations::host`: that module declares
//! what the engine needs from the app around it, and this is where the app
//! answers.
//!
//! - runtime — the volume host's (`volume_host::host()`), which is the app's own
//!   tokio runtime, so an admitted operation never runs on a caller's
//! - volumes — [`volumes::AppOperationVolumes`], the volume registry asked per
//!   call
//! - engine-wide events — [`events::TauriEngineEvents`]
//! - own writes — [`own_writes::AppOwnWrites`], the downloads watcher's ignore
//!   set
//! - one operation's or preview's events — [`TauriEventSink`] and
//!   [`TauriScanPreviewSink`] (`sinks.rs`), built at the IPC edge; a terminal
//!   event's app side effects ride along (`terminal_effects.rs`)
//! - the wire — `wire.rs` wraps each engine payload as the Tauri event it
//!   travels as

mod analytics;
mod events;
mod own_writes;
mod sinks;
pub(crate) mod terminal_effects;
mod volumes;
// Which app side effects each terminal event fires, and in what order around its emit.
#[cfg(test)]
mod terminal_effects_tests;
pub(crate) mod wire;

use std::sync::Arc;

use tauri::AppHandle;

use crate::file_system::write_operations::OperationEventSink;
use crate::file_system::write_operations::host::{self, EngineHost};
pub(crate) use events::VolumesBusyChanged;
pub use sinks::{TauriEventSink, TauriScanPreviewSink};

/// Wire the write engine to this app. Call once, in `setup()`, after
/// `volume_host::install` (the engine's runtime is the volume host's) and
/// before anything can start an operation or the leftover sweep runs.
pub fn install(app: &AppHandle) {
    let sink_app = app.clone();
    let engine_host = app_answers(Some(app.clone())).with_operation_sinks(Box::new(move || {
        Arc::new(TauriEventSink::new(sink_app.clone())) as Arc<dyn OperationEventSink>
    }));
    host::install(engine_host);
}

/// The app's answers with no frontend behind them, for a test binary, which
/// never installs.
#[cfg(test)]
pub(crate) fn without_frontend() -> EngineHost {
    app_answers(None)
}

/// Every seam the app answers the same with or without a frontend; `app` is
/// where the engine-wide events go out, if anywhere.
fn app_answers(app: Option<AppHandle>) -> EngineHost {
    EngineHost::new(
        crate::volume_host::host(),
        Arc::new(volumes::AppOperationVolumes),
        Arc::new(events::TauriEngineEvents::new(app)),
    )
    .with_own_writes(Arc::new(own_writes::AppOwnWrites))
}
