//! Everything the listing engine needs from the application around it, as named
//! traits instead of reaches upward.
//!
//! Each seam answers one question, degrades to a no-op (the public `No*` unit
//! types), and is answered app-side by one adapter in `crate::listing_host`.
//! The engine reports facts and asks questions; what the user sees, which
//! volume serves a path, and what the index knows are the host's.
//!
//! - `events`: what happened to a listing ([`events::ListingEvents`]).
//! - `volumes`: which volume serves a path ([`volumes::ListingVolumes`]).
//! - `index`: what the drive index knows ([`index::ListingIndex`]).
//! - `overlays`: rows a pane sees that no volume holds ([`overlays::ListingOverlay`]).
//! - `observers`: what keeps something alive while a pane shows a directory
//!   ([`observers::ListingObserver`]).
//! - `leases`: the user is waiting on this volume ([`leases::ForegroundLeases`]).
//! - `locale`: which language names collate in ([`locale::ReadingLocale`]).
//! - `mounts`: what a filesystem path is mounted on ([`mounts::MountTable`]).
//! - The runtime: a [`tokio::runtime::Handle`], not a trait. ❌ Never
//!   `tokio::spawn` in the engine: it INHERITS an ambient runtime, and a watcher's
//!   OS thread has none, so it panics there. [`runtime`] RESOLVES one.
//!
//! The installed seams live in one process-wide slot that the app fills once at
//! startup (`crate::listing_host::install`). The engine reads them per call
//! through the accessors here. ❌ A seam is called per read or per mutation,
//! never per entry: every one is a `dyn` call.

#![expect(
    dead_code,
    reason = "issue #301: the seams arrive in their final shape before M2b and M2c route the engine through all of them"
)]

pub mod events;
pub mod index;
pub mod leases;
pub mod locale;
pub mod mounts;
pub mod observers;
pub mod overlays;
#[cfg(test)]
pub(crate) mod testing;
pub mod volumes;

use std::sync::{Arc, OnceLock};

use tokio::runtime::Handle;

pub use events::{ListingEnd, ListingEvents, NoEvents};

/// The seams one application installs, as one value.
pub struct ListingSeams {
    events: Arc<dyn ListingEvents>,
    runtime: Handle,
}

impl ListingSeams {
    /// Seams that report to `events` and spawn onto the shared fallback runtime
    /// until [`runtime`](Self::runtime) names the host's own.
    pub fn new(events: Arc<dyn ListingEvents>) -> Self {
        Self {
            events,
            runtime: cmdr_fs::volume::host::VolumeHost::detached().runtime(),
        }
    }

    /// Spawns the engine's background work onto `handle`, so the host keeps ONE
    /// thread pool.
    pub fn runtime(mut self, handle: Handle) -> Self {
        self.runtime = handle;
        self
    }

    /// Seams that answer nothing: events dropped, the fallback runtime.
    pub fn detached() -> Self {
        Self::new(Arc::new(NoEvents))
    }
}

/// The installed seams.
static INSTALLED: OnceLock<ListingSeams> = OnceLock::new();

/// Why [`install`] reported trouble.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// Seams were installed already; the first ones stay.
    AlreadyInstalled,
    /// The new seams are installed, but something read the slot first and got
    /// detached answers: a listing event before startup finished went nowhere.
    ReadBeforeInstall,
}

/// Fills the slot. Call once, at startup, before anything can list.
pub fn install(seams: ListingSeams) -> Result<(), InstallError> {
    INSTALLED.set(seams).map_err(|_| InstallError::AlreadyInstalled)?;
    if early_reads::happened() {
        return Err(InstallError::ReadBeforeInstall);
    }
    Ok(())
}

/// The installed seams, or the detached ones.
///
/// In a test binary nothing installs, so the first read claims the slot with
/// detached seams, quietly. In the app a read before [`install`] is a bug that
/// would leave every pane on "loading", so it's loud, and it answers from a
/// SEPARATE detached value: the slot stays free for [`install`], which then
/// reports the early read.
fn seams() -> &'static ListingSeams {
    if let Some(installed) = INSTALLED.get() {
        return installed;
    }
    #[cfg(test)]
    {
        INSTALLED.get_or_init(ListingSeams::detached)
    }
    #[cfg(not(test))]
    {
        early_reads::note()
    }
}

/// Where the engine reports what happened to a listing.
pub(crate) fn events() -> Arc<dyn ListingEvents> {
    Arc::clone(&seams().events)
}

/// The runtime the engine spawns onto: the host's own, or the shared fallback.
pub(crate) fn runtime() -> &'static Handle {
    &seams().runtime
}

#[cfg(not(test))]
mod early_reads {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::ListingSeams;

    static HAPPENED: AtomicBool = AtomicBool::new(false);
    static DETACHED: OnceLock<ListingSeams> = OnceLock::new();

    pub(super) fn note() -> &'static ListingSeams {
        HAPPENED.store(true, Ordering::Relaxed);
        // A warning here, and the error report from the host's `install`, which
        // hears about it through `InstallError::ReadBeforeInstall`.
        log::warn!(target: "listing", "the listing engine's seams were read before install; answering detached");
        DETACHED.get_or_init(ListingSeams::detached)
    }

    pub(super) fn happened() -> bool {
        HAPPENED.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod early_reads {
    pub(super) fn happened() -> bool {
        false
    }
}
