//! This app, acting as the write engine's host.
//!
//! The mirror of `file_system::write_operations::host`: that module declares
//! what the engine needs from the app around it, and this is where the app
//! answers.
//!
//! - runtime — the volume host's (`volume_host::host()`), which is the app's own
//!   tokio runtime, so an admitted operation never runs on a caller's
//! - engine-wide events — [`events::TauriEngineEvents`]

mod events;

use std::sync::Arc;

use tauri::AppHandle;

use crate::file_system::write_operations::host::{self, EngineHost};

/// Wire the write engine to this app. Call once, in `setup()`, after
/// `volume_host::install` (the engine's runtime is the volume host's) and
/// before anything can start an operation or the leftover sweep runs.
pub fn install(app: &AppHandle) {
    let engine_host = EngineHost::new(
        crate::volume_host::host(),
        Arc::new(events::TauriEngineEvents::new(Some(app.clone()))),
    )
    .with_app(app.clone());
    host::install(engine_host);
}

/// The app's answers with no frontend behind them, for a test binary, which
/// never installs.
#[cfg(test)]
pub(crate) fn without_frontend() -> EngineHost {
    EngineHost::new(
        crate::volume_host::host(),
        Arc::new(events::TauriEngineEvents::new(None)),
    )
}
