//! This app, acting as the listing engine's host.
//!
//! The mirror of `file_system::listing::host`: that module declares everything
//! the listing engine needs from the application around it, and this is the
//! place that answers, once, at the top of `setup()`. Each answer is an adapter
//! here or next to the subsystem that gives it.
//!
//! - events: [`events::TauriListingEvents`], the only place the engine's facts
//!   become wire events and the bookkeeping around them
//! - runtime: the app's own tokio handle, so there's one thread pool
//!
//! The other direction lives here too: [`backends::AppListings`] answers a
//! storage backend's `cmdr_fs` `ListingHost` questions from the engine.

pub(crate) mod backends;
pub(crate) mod events;

use std::sync::Arc;

use tauri::AppHandle;

use crate::file_system::listing::host::{self, InstallError, ListingSeams};

/// Wire the listing engine to this app: its events and its runtime.
///
/// Call first thing in `setup()`, before anything can open a listing. An
/// adapter resolves the subsystem it answers from per call, never here: this
/// runs before `index_host::install`, and capturing `index()` now would win
/// its slot with an unconfigured index.
pub fn install(app: &AppHandle) {
    let seams = ListingSeams::new(Arc::new(events::TauriListingEvents::new(app.clone())))
        // The app's own runtime, so a listing's background work shares one
        // thread pool with everything else, and a watcher's OS thread (no
        // reactor) can still start a refresh.
        .runtime(tauri::async_runtime::handle().inner().clone());
    match host::install(seams) {
        Ok(()) => {}
        Err(InstallError::AlreadyInstalled) => {
            crate::log_error!(target: "listing", "the listing host was installed twice; keeping the first");
        }
        // Something read the seams before this ran and got detached answers: a
        // listing event before startup finished went nowhere.
        Err(InstallError::ReadBeforeInstall) => {
            crate::log_error!(target: "listing", "the listing host was read before install(); early events were dropped");
        }
    }
}
