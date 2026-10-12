//! The listing engine as one value: [`Listings`], the handle that owns every
//! listing, every in-flight read, every pending diff, and the seams it asks the
//! host through.
//!
//! The host builds one with [`Listings::builder`] and keeps it; a test or a tool
//! builds its own with [`Listings::detached`]. Two engines share nothing: a
//! listing one of them holds is invisible to the other.
//!
//! Every engine operation lives on [`Inner`], in the module that owns that part
//! of the engine (`caching.rs`, `operations.rs`, `streaming.rs`, …), as an
//! `impl Inner` block. ❌ Engine code reaches its state through `self`, never
//! through the app's accessor: a method that asks for "the" engine works in the
//! app and quietly reads the wrong one everywhere else.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::Instant;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use tokio::runtime::Handle;

use super::cached_listing::{CachedListing, ListingPath};
use super::caching::RefreshSlot;
use super::collation::NameCollator;
use super::diff_emitter::DiffQueue;
use super::host::{
    ForegroundLeases, ListingEvents, ListingIndex, ListingObserver, ListingOverlay, ListingVolumes, MountTable,
    NoEvents, NoIndex, NoLeases, NoLocale, NoMountTable, NoVolumes, ReadingLocale, ScratchVisibility,
};
use super::stall::{HungReads, StallPolicy, StallWatch};
use super::streaming::StreamingListingState;

mod backends;
mod housekeeping;
mod intake;
mod open;
mod oracle;
mod rows;
mod settings;

pub use settings::PaneMarks;

/// The listing engine: every open listing, and the seams it asks the host
/// through. Cheap to clone; every clone is the same engine.
#[derive(Clone)]
pub struct Listings {
    pub(super) inner: Arc<Inner>,
}

impl Listings {
    /// Starts an engine that lists through `volumes` and reports to `events`, the
    /// two seams no real host can do without: with no volumes it lists nothing,
    /// and with no events every pane waits on "loading" forever. The other seams
    /// default to their no-op answers.
    pub fn builder(volumes: Arc<dyn ListingVolumes>, events: Arc<dyn ListingEvents>) -> ListingsBuilder {
        ListingsBuilder::new(volumes, events)
    }

    /// An engine that answers nothing: no volumes, events dropped, every other
    /// seam a no-op, the shared fallback runtime.
    pub fn detached() -> Self {
        Self::builder(Arc::new(NoVolumes), Arc::new(NoEvents)).build()
    }

    /// The engine's state and operations, for the test shims that still forward
    /// to it. Shim: removed in M4b, when every test builds its own engine.
    #[cfg(test)]
    pub(crate) fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }
}

/// Builds a [`Listings`]. Every seam it isn't handed answers with its no-op.
pub struct ListingsBuilder {
    events: Arc<dyn ListingEvents>,
    volumes: Arc<dyn ListingVolumes>,
    index: Arc<dyn ListingIndex>,
    leases: Arc<dyn ForegroundLeases>,
    locale: Arc<dyn ReadingLocale>,
    mounts: Arc<dyn MountTable>,
    runtime: Handle,
    overlays: Vec<Arc<dyn ListingOverlay>>,
    observers: Vec<Arc<dyn ListingObserver>>,
}

impl ListingsBuilder {
    fn new(volumes: Arc<dyn ListingVolumes>, events: Arc<dyn ListingEvents>) -> Self {
        Self {
            events,
            volumes,
            index: Arc::new(NoIndex),
            leases: Arc::new(NoLeases),
            locale: Arc::new(NoLocale),
            mounts: Arc::new(NoMountTable),
            runtime: cmdr_fs::volume::host::VolumeHost::detached().runtime(),
            overlays: Vec::new(),
            observers: Vec::new(),
        }
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
    /// overlay added before it. The first overlay per id wins.
    pub fn overlay(mut self, overlay: Arc<dyn ListingOverlay>) -> Self {
        add_overlay(&mut self.overlays, overlay);
        self
    }

    /// Tells `observer` about every listing open and close, after every
    /// observer added before it. The first observer per id wins.
    pub fn observer(mut self, observer: Arc<dyn ListingObserver>) -> Self {
        add_observer(&mut self.observers, observer);
        self
    }

    /// Spawns the engine's background work onto `handle`, so the host keeps ONE
    /// thread pool.
    pub fn runtime(mut self, handle: Handle) -> Self {
        self.runtime = handle;
        self
    }

    /// The engine, with no listing open.
    pub fn build(self) -> Listings {
        let diffs = Arc::new(DiffQueue::new(Arc::clone(&self.events), self.runtime.clone()));
        Listings {
            inner: Arc::new(Inner {
                events: self.events,
                volumes: self.volumes,
                index: self.index,
                leases: self.leases,
                locale: self.locale,
                mounts: self.mounts,
                runtime: self.runtime,
                overlays: RwLock::new(self.overlays),
                observers: RwLock::new(self.observers),
                scratch: ScratchSettings::new(ScratchVisibility::default()),
                cache: RwLock::new(HashMap::new()),
                epoch: Instant::now(),
                streaming: RwLock::new(HashMap::new()),
                diffs,
                refresh_slots: RwLock::new(HashMap::new()),
                read_generations: AtomicU64::new(0),
                hung_reads: Arc::new(HungReads::new()),
                collators: RwLock::new(HashMap::new()),
            }),
        }
    }
}

/// The first overlay per id wins; a duplicate is logged and dropped, so a
/// double registration can't contribute the same rows twice.
fn add_overlay(overlays: &mut Vec<Arc<dyn ListingOverlay>>, overlay: Arc<dyn ListingOverlay>) {
    if overlays.iter().any(|o| o.id() == overlay.id()) {
        log::warn!(target: "listing", "listing overlay {} registered twice; keeping the first", overlay.id());
        return;
    }
    log::debug!(target: "listing", "registered listing overlay {}", overlay.id());
    overlays.push(overlay);
}

/// The first observer per id wins, so a double registration can't arm twice.
fn add_observer(observers: &mut Vec<Arc<dyn ListingObserver>>, observer: Arc<dyn ListingObserver>) {
    if observers.iter().any(|o| o.id() == observer.id()) {
        return;
    }
    log::debug!(target: "listing", "registered listing observer {}", observer.id());
    observers.push(observer);
}

/// The engine's state: the seams it asks, and everything it keeps between two
/// calls. One per [`Listings`]; nothing here is process-wide.
pub(crate) struct Inner {
    /// Where the engine reports what happened to a listing.
    pub(super) events: Arc<dyn ListingEvents>,
    /// Which volume serves a path.
    pub(super) volumes: Arc<dyn ListingVolumes>,
    /// What the drive index knows.
    pub(super) index: Arc<dyn ListingIndex>,
    /// Who hands out foreground holds.
    pub(super) leases: Arc<dyn ForegroundLeases>,
    /// Which language names collate in.
    locale: Arc<dyn ReadingLocale>,
    /// What a filesystem path is mounted on.
    pub(super) mounts: Arc<dyn MountTable>,
    /// The runtime every spawn goes to. ❌ Never `tokio::spawn` in the engine:
    /// it INHERITS an ambient runtime, and a watcher's OS thread has none, so it
    /// panics there.
    pub(super) runtime: Handle,
    /// Behind a lock only so a test can add one to the shared engine until each
    /// test builds its own (M4b); the host hands its own over at build.
    overlays: RwLock<Vec<Arc<dyn ListingOverlay>>>,
    /// Same as `overlays`.
    observers: RwLock<Vec<Arc<dyn ListingObserver>>>,
    /// The scratch files the user asked to see.
    scratch: ScratchSettings,
    /// Every open listing, by listing id.
    pub(super) cache: RwLock<HashMap<String, CachedListing>>,
    /// What a listing's `last_accessed_ms` counts from.
    pub(super) epoch: Instant,
    /// Every streaming read still running, by listing id: its cancel token and
    /// stall watch.
    pub(super) streaming: RwLock<HashMap<String, Arc<StreamingListingState>>>,
    /// Committed diff batches waiting for their coalesced flush.
    pub(super) diffs: Arc<DiffQueue>,
    /// Per-directory refresh turnstiles (`caching.rs`).
    pub(super) refresh_slots: RwLock<HashMap<(String, PathBuf), RefreshSlot>>,
    /// The last read generation handed out (`reread.rs`).
    pub(super) read_generations: AtomicU64,
    /// Every volume's listing reads, and which of them are hung (`stall.rs`).
    hung_reads: Arc<HungReads>,
    /// One collator per locale tag (`collation.rs`).
    pub(super) collators: RwLock<HashMap<String, Arc<NameCollator>>>,
}

impl Inner {
    /// Which scratch files the user asked to see, read once per operation and
    /// passed down as a value, ❗ before any cache lock is taken.
    pub(crate) fn scratch_visibility(&self) -> ScratchVisibility {
        self.scratch.get()
    }

    /// Milliseconds since this engine's epoch: what `last_accessed_ms` holds.
    pub(crate) fn epoch_millis_now(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// `path` in the spelling `volume_id`'s volume keys listings under.
    ///
    /// ❗ Call it before taking the cache: it asks the host's volume seam.
    pub(crate) fn listing_path(&self, volume_id: &str, path: &std::path::Path) -> ListingPath {
        ListingPath::on_volume(self.volumes.as_ref(), volume_id, path)
    }

    /// The runtime every spawn goes to.
    #[cfg(test)]
    pub(crate) fn runtime(&self) -> &Handle {
        &self.runtime
    }

    /// Which language names collate in, as the host answers it now.
    pub(super) fn locale(&self) -> String {
        self.locale.locale()
    }

    /// The stall policy production reads use, and this engine's registry of
    /// hung reads.
    pub(super) fn stall_watch(&self) -> StallWatch {
        StallWatch {
            policy: StallPolicy::PRODUCTION,
            reads: Arc::clone(&self.hung_reads),
        }
    }

    /// A snapshot of the overlays, in registration order, so none runs under the
    /// list's lock.
    pub(super) fn overlays(&self) -> Vec<Arc<dyn ListingOverlay>> {
        self.overlays.read_ignore_poison().clone()
    }

    /// A snapshot of the observers, in registration order, so none runs under
    /// the list's lock.
    pub(super) fn observers(&self) -> Vec<Arc<dyn ListingObserver>> {
        self.observers.read_ignore_poison().clone()
    }

    /// Adds `overlay` after every overlay already there. Test registration on
    /// the shared engine; the host hands its own to the builder.
    #[cfg(test)]
    pub(crate) fn add_overlay(&self, overlay: Arc<dyn ListingOverlay>) {
        add_overlay(&mut self.overlays.write_ignore_poison(), overlay);
    }

    /// Adds `observer` after every observer already there. Test registration on
    /// the shared engine; the host hands its own to the builder.
    #[cfg(test)]
    pub(crate) fn add_observer(&self, observer: Arc<dyn ListingObserver>) {
        add_observer(&mut self.observers.write_ignore_poison(), observer);
    }

    /// A weak handle, for a background loop that must not keep the engine alive.
    pub(super) fn downgrade(self: &Arc<Self>) -> Weak<Self> {
        Arc::downgrade(self)
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
