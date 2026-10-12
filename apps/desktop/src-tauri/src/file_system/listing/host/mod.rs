//! Everything the listing engine needs from the application around it, as named
//! traits instead of reaches upward.
//!
//! Each seam answers one question, degrades to a no-op (the public `No*` unit
//! types), and is answered app-side by one adapter in the app's `listing_host` module.
//! The engine reports facts and asks questions; what the user sees, which
//! volume serves a path, and what the index knows are the host's.
//!
//! - `events`: what happened to a listing ([`events::ListingEvents`]).
//! - `volumes`: which volume serves a path ([`volumes::ListingVolumes`]).
//! - `index`: what the drive index knows ([`index::ListingIndex`]).
//! - `overlays`: rows a pane sees that no volume holds ([`overlays::ListingOverlay`]),
//!   a list in registration order.
//! - `observers`: what keeps something alive while a pane shows a directory
//!   ([`observers::ListingObserver`]), a list in registration order.
//! - `leases`: the user is waiting on this volume ([`leases::ForegroundLeases`]).
//! - `locale`: which language names collate in ([`locale::ReadingLocale`]).
//! - `mounts`: what a filesystem path is mounted on ([`mounts::MountTable`]).
//! - The scratch visibility: engine STATE the host sets, not a seam
//!   ([`ScratchVisibility`]); see [`set_scratch_visibility`].
//! - The runtime: a [`tokio::runtime::Handle`], not a trait. ❌ Never
//!   `tokio::spawn` in the engine: it INHERITS an ambient runtime, and a watcher's
//!   OS thread has none, so it panics there. [`runtime`] RESOLVES one.
//!
//! The installed seams live in one process-wide slot that the app fills once at
//! startup (the app's `listing_host::install`). The engine reads them per call
//! through the accessors here. ❌ A seam is called per read or per mutation,
//! never per entry: every one is a `dyn` call.

pub mod events;
pub mod index;
pub mod leases;
pub mod locale;
pub mod mounts;
pub mod observers;
pub mod overlays;
#[cfg(test)]
mod overlays_test;
#[cfg(test)]
pub(crate) mod testing;
pub mod volumes;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use tokio::runtime::Handle;

pub use events::{ListingEnd, ListingEvents, NoEvents};
pub use index::{ListingIndex, NoIndex};
pub use leases::{ForegroundHold, ForegroundLeases, NoLeases};
pub use locale::{NoLocale, ReadingLocale};
pub use mounts::{MountFacts, MountTable, NoMountTable};
pub use observers::{ListingObserver, OpenedListing};
pub use overlays::ListingOverlay;
pub use volumes::{ListingVolumes, NoVolumes, Unregistered};

/// The seams one application installs, as one value.
pub struct ListingSeams {
    events: Arc<dyn ListingEvents>,
    volumes: Arc<dyn ListingVolumes>,
    index: Arc<dyn ListingIndex>,
    leases: Arc<dyn ForegroundLeases>,
    locale: Arc<dyn ReadingLocale>,
    mounts: Arc<dyn MountTable>,
    runtime: Handle,
    /// Behind a lock only so a test can add one to the shared slot; the host
    /// hands its own over before install.
    overlays: RwLock<Vec<Arc<dyn ListingOverlay>>>,
    /// Same as `overlays`.
    observers: RwLock<Vec<Arc<dyn ListingObserver>>>,
    /// Engine state, not a seam: the host sets it, the engine reads it.
    scratch: ScratchSettings,
}

impl ListingSeams {
    /// Seams that report to `events` and spawn onto the shared fallback runtime
    /// until [`runtime`](Self::runtime) names the host's own.
    pub fn new(events: Arc<dyn ListingEvents>) -> Self {
        Self {
            events,
            volumes: Arc::new(NoVolumes),
            index: Arc::new(NoIndex),
            leases: Arc::new(NoLeases),
            locale: Arc::new(NoLocale),
            mounts: Arc::new(NoMountTable),
            runtime: cmdr_fs::volume::host::VolumeHost::detached().runtime(),
            overlays: RwLock::new(Vec::new()),
            observers: RwLock::new(Vec::new()),
            scratch: ScratchSettings::new(ScratchVisibility::default()),
        }
    }

    /// Asks `volumes` which volume serves a path.
    pub fn volumes(mut self, volumes: Arc<dyn ListingVolumes>) -> Self {
        self.volumes = volumes;
        self
    }

    /// Asks `index` what the drive index knows about a listing's rows.
    pub fn index(mut self, index: Arc<dyn ListingIndex>) -> Self {
        self.index = index;
        self
    }

    /// Takes a foreground hold from `leases` for every streaming read.
    pub fn leases(mut self, leases: Arc<dyn ForegroundLeases>) -> Self {
        self.leases = leases;
        self
    }

    /// Collates names in the language `locale` answers.
    pub fn locale(mut self, locale: Arc<dyn ReadingLocale>) -> Self {
        self.locale = locale;
        self
    }

    /// Reads what a stalled folder is mounted on from `mounts`.
    pub fn mounts(mut self, mounts: Arc<dyn MountTable>) -> Self {
        self.mounts = mounts;
        self
    }

    /// Folds `overlay`'s rows into the listings it applies to, after every
    /// overlay added before it.
    pub fn overlay(self, overlay: Arc<dyn ListingOverlay>) -> Self {
        self.add_overlay(overlay);
        self
    }

    /// Tells `observer` about every listing open and close, after every
    /// observer added before it.
    pub fn observer(self, observer: Arc<dyn ListingObserver>) -> Self {
        self.add_observer(observer);
        self
    }

    /// The first overlay per id wins; a duplicate is logged and dropped, so a
    /// double registration can't contribute the same rows twice.
    fn add_overlay(&self, overlay: Arc<dyn ListingOverlay>) {
        let mut overlays = self.overlays.write_ignore_poison();
        if overlays.iter().any(|o| o.id() == overlay.id()) {
            log::warn!(target: "listing", "listing overlay {} registered twice; keeping the first", overlay.id());
            return;
        }
        log::debug!(target: "listing", "registered listing overlay {}", overlay.id());
        overlays.push(overlay);
    }

    /// The first observer per id wins, so a double registration can't arm twice.
    fn add_observer(&self, observer: Arc<dyn ListingObserver>) {
        let mut observers = self.observers.write_ignore_poison();
        if observers.iter().any(|o| o.id() == observer.id()) {
            return;
        }
        log::debug!(target: "listing", "registered listing observer {}", observer.id());
        observers.push(observer);
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
        INSTALLED.get_or_init(testing::fallback_seams)
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

/// Which volume serves a path.
pub(crate) fn volumes() -> Arc<dyn ListingVolumes> {
    Arc::clone(&seams().volumes)
}

/// What the drive index knows.
pub(crate) fn index() -> Arc<dyn ListingIndex> {
    Arc::clone(&seams().index)
}

/// Who hands out foreground holds.
pub(crate) fn leases() -> Arc<dyn ForegroundLeases> {
    Arc::clone(&seams().leases)
}

/// Which language names collate in.
pub(crate) fn locale() -> Arc<dyn ReadingLocale> {
    Arc::clone(&seams().locale)
}

/// What a filesystem path is mounted on.
pub(crate) fn mounts() -> Arc<dyn MountTable> {
    Arc::clone(&seams().mounts)
}

/// Which scratch files the user asked to see, read once per question.
pub fn scratch_visibility() -> ScratchVisibility {
    seams().scratch.get()
}

/// Applies the user's scratch-visibility settings to every listing from its next
/// read on; a listing already cached reconciles lazily, at its next read or
/// mutation (`visible_rows.rs`).
pub fn set_scratch_visibility(visibility: ScratchVisibility) {
    seams().scratch.set(visibility);
}

/// Which scratch files a pane shows, as the user set it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScratchVisibility {
    /// Cmdr's own scratch (`advanced.showStagingTempFiles`). Off by default.
    pub show_staging_temps: bool,
    /// Other apps' safe-save scratch (`advanced.showSafeSaveFiles`). On by
    /// default: hiding another app's files by name is a bigger claim to make on
    /// a user's behalf than hiding Cmdr's own.
    pub show_safe_save_files: bool,
}

impl Default for ScratchVisibility {
    fn default() -> Self {
        Self {
            show_staging_temps: false,
            show_safe_save_files: true,
        }
    }
}

/// [`ScratchVisibility`] behind two atomics, so a read never takes a lock.
struct ScratchSettings {
    show_staging_temps: AtomicBool,
    show_safe_save_files: AtomicBool,
}

impl ScratchSettings {
    fn new(visibility: ScratchVisibility) -> Self {
        Self {
            show_staging_temps: AtomicBool::new(visibility.show_staging_temps),
            show_safe_save_files: AtomicBool::new(visibility.show_safe_save_files),
        }
    }

    fn get(&self) -> ScratchVisibility {
        ScratchVisibility {
            show_staging_temps: self.show_staging_temps.load(Ordering::Relaxed),
            show_safe_save_files: self.show_safe_save_files.load(Ordering::Relaxed),
        }
    }

    fn set(&self, visibility: ScratchVisibility) {
        self.show_staging_temps
            .store(visibility.show_staging_temps, Ordering::Relaxed);
        self.show_safe_save_files
            .store(visibility.show_safe_save_files, Ordering::Relaxed);
    }
}

/// The runtime the engine spawns onto: the host's own, or the shared fallback.
pub(crate) fn runtime() -> &'static Handle {
    &seams().runtime
}

/// A snapshot of the overlays, in registration order, so none runs under the
/// slot's lock.
pub(crate) fn overlays() -> Vec<Arc<dyn ListingOverlay>> {
    seams().overlays.read_ignore_poison().clone()
}

/// A snapshot of the observers, in registration order, so none runs under the
/// slot's lock.
pub(crate) fn observers() -> Vec<Arc<dyn ListingObserver>> {
    seams().observers.read_ignore_poison().clone()
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
