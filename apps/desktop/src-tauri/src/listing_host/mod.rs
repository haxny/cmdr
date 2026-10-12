//! This app, acting as the listing engine's host.
//!
//! The mirror of `cmdr_listing::host`: that module declares everything
//! the listing engine needs from the application around it, and this is the
//! place that answers, once, at the top of `setup()`, by building the app's one
//! engine ([`listings`]). Each answer is an adapter here or next to the subsystem
//! that gives it.
//!
//! - events: [`events::TauriListingEvents`], the only place the engine's facts
//!   become wire events and the bookkeeping around them
//! - volumes: [`volumes::AppVolumes`] over the `VolumeManager`, mount adoption,
//!   and `unregistered_volumes`
//! - index: [`index::AppListingIndex`] over `index_host::index()`
//! - leases: [`leases::AppLeases`] over `priority::foreground`
//! - locale: [`locale::AppLocale`] over `intl::active_locale`
//! - mounts: [`mounts::AppMountTable`] over `getfsstat` / `/proc/mounts`
//! - runtime: the app's own tokio handle, so there's one thread pool
//! - overlays: the git portal's `.git/` rows (`file_system::git::overlay`)
//! - observers, in this order: the change watch (`file_system::watcher`), git
//!   arming (`file_system::git::arming`), then index sizes
//!   (`listing_index_sizes`)
//!
//! The other direction lives here too: [`backends::AppListings`] answers a
//! storage backend's `cmdr_fs` `ListingHost` questions from the engine, and
//! [`directory_changed`] is where every directory change enters, in the one
//! order the subsystems hearing it need.

pub(crate) mod backends;
pub(crate) mod events;
pub(crate) mod index;
pub(crate) mod leases;
pub(crate) mod locale;
pub(crate) mod mounts;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod volumes;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use cmdr_fs::volume::DirectoryChange;

use tauri::AppHandle;

use cmdr_listing::host::{ListingEvents, ListingObserver};
use cmdr_listing::{Listings, ListingsBuilder};

/// The app's listing engine, once [`install`] built it.
static LISTINGS: OnceLock<Listings> = OnceLock::new();

/// Builds the app's listing engine, wired to this app: its events, its runtime,
/// its overlays, and its observers.
///
/// Call first thing in `setup()`, before anything can open a listing. An
/// adapter resolves the subsystem it answers from per call, never here: this
/// runs before `index_host::install`, and capturing `index()` now would win
/// its slot with an unconfigured index.
pub fn install(app: &AppHandle) {
    let listings = observers()
        .into_iter()
        .fold(
            builder(Arc::new(events::TauriListingEvents::new(app.clone())))
                // The app's own runtime, so a listing's background work shares one
                // thread pool with everything else, and a watcher's OS thread (no
                // reactor) can still start a refresh.
                .runtime(tauri::async_runtime::handle().inner().clone())
                // The portal's `.git/` landing rows. Everything below `.git/` is a
                // routed volume instead; this seam is what keeps the six rows out of
                // every walker.
                .overlay(crate::file_system::git::overlay::overlay()),
            ListingsBuilder::observer,
        )
        .build();
    if LISTINGS.set(listings).is_err() {
        crate::log_error!(target: "listing", "the listing host was installed twice; keeping the first");
    }
    // Something read the engine before this ran and got a detached one: a
    // listing event before startup finished went nowhere.
    if early_reads::happened() {
        crate::log_error!(target: "listing", "the listing engine was read before install(); early events were dropped");
    }
}

/// The app's observers, in the order every listing open and close reaches them.
fn observers() -> [Arc<dyn ListingObserver>; 3] {
    [
        // FIRST: arms a listing's change watch on open and disarms it on close,
        // after the cache removal like every observer.
        crate::file_system::watcher::observer(),
        // Keeps a repo's `.git/*` watcher armed for as long as a pane shows one
        // of its virtual listings, so a lone `branches/` pane stays live.
        crate::file_system::git::arming::observer(),
        // Routes the index's folder-size updates to the listings they touch.
        crate::listing_index_sizes::observer(),
    ]
}

/// The app's listing engine.
///
/// In a test binary nothing installs, so the first read builds the test engine
/// (`test_listings`) and keeps it, quietly. In the app a read before
/// [`install`] is a bug that would leave every pane on "loading", so it's loud,
/// and it answers from a SEPARATE detached engine: the slot stays free for
/// [`install`], which then reports the early read.
pub(crate) fn listings() -> &'static Listings {
    if let Some(listings) = LISTINGS.get() {
        return listings;
    }
    #[cfg(test)]
    {
        LISTINGS.get_or_init(test_listings)
    }
    #[cfg(not(test))]
    {
        early_reads::note()
    }
}

/// An engine with the app's answers to its questions (which volume, what the
/// index knows, the foreground lease, the reading language, and the mount
/// table), reporting to `events`.
fn builder(events: Arc<dyn ListingEvents>) -> ListingsBuilder {
    Listings::builder(Arc::new(volumes::AppVolumes), events)
        .index(Arc::new(index::AppListingIndex))
        .leases(Arc::new(leases::AppLeases))
        .locale(Arc::new(locale::AppLocale))
        .mounts(Arc::new(mounts::AppMountTable))
}

/// The engine the test binary shares, since nothing there calls [`install`]:
/// events dropped, the fallback runtime, every app answer (the tests list
/// through the real volume registry, index, lease, language, and mount table),
/// the git portal's overlay, and the change watch and git arming observers, so
/// a listing behaves in tests as it does in the app. Index sizes stay out (its
/// worker needs an `AppHandle`); last comes the lifecycle probe the app's
/// ordering cells read (`tests/support.rs`).
#[cfg(test)]
fn test_listings() -> Listings {
    builder(Arc::new(cmdr_listing::host::NoEvents))
        .overlay(crate::file_system::git::overlay::overlay())
        .observer(crate::file_system::watcher::observer())
        .observer(crate::file_system::git::arming::observer())
        .observer(tests::support::lifecycle_probe())
        .build()
}

/// An engine of a test's own with every app answer to its questions (the real
/// volume registry, index, lease, language, and mount table), reporting to
/// `events`: for a test that reads what the engine reported, which the shared
/// test engine drops. No observers, so nothing arms a watch.
#[cfg(test)]
pub(crate) fn engine_reporting_to(events: Arc<dyn ListingEvents>) -> Listings {
    builder(events).build()
}

/// A directory changed on a volume: tells every subsystem that keeps a copy of
/// it, in order.
///
/// 1. The drive index FIRST, before any pane work. The SMB watcher runs for the
///    WHOLE volume's lifetime, not just while a pane shows the share, so the
///    index must update even when no listing shows this path. And writing it
///    before the panes' enrich means the enrich (and the `DirsUpdated` the
///    writer emits) see the just-written sizes, not the pre-event ones. A no-op
///    for `root` and any non-indexed volume.
/// 2. The cloud badge, which caches per directory, so this is how it learns a
///    file it has an answer for moved on. One hash lookup, hit or miss.
/// 3. The panes (`Listings::directory_changed`).
///
/// The coupling is one-directional: listing → indexer, never the reverse.
pub(crate) fn directory_changed(volume_id: &str, parent_path: &Path, change: DirectoryChange) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::index_host::index().apply_directory_change(volume_id, parent_path, &change);

    #[cfg(target_os = "macos")]
    crate::file_system::sync_status::invalidate_dir(parent_path);

    listings().directory_changed(volume_id, parent_path, change);
}

#[cfg(not(test))]
mod early_reads {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use cmdr_listing::Listings;

    static HAPPENED: AtomicBool = AtomicBool::new(false);
    static DETACHED: OnceLock<Listings> = OnceLock::new();

    pub(super) fn note() -> &'static Listings {
        if !HAPPENED.swap(true, Ordering::Relaxed) {
            crate::log_error!(target: "listing", "the listing engine was read before listing_host::install(); answering detached");
        }
        DETACHED.get_or_init(Listings::detached)
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
