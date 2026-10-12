//! This app, acting as the listing engine's host.
//!
//! The mirror of `file_system::listing::host`: that module declares everything
//! the listing engine needs from the application around it, and this is the
//! place that answers, once, at the top of `setup()`. Each answer is an adapter
//! here or next to the subsystem that gives it.
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
pub(crate) mod volumes;

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::DirectoryChange;

use tauri::AppHandle;

use crate::file_system::listing::host::{self, InstallError, ListingSeams};

/// Wire the listing engine to this app: its events, its runtime, its overlays,
/// and its observers.
///
/// Call first thing in `setup()`, before anything can open a listing. An
/// adapter resolves the subsystem it answers from per call, never here: this
/// runs before `index_host::install`, and capturing `index()` now would win
/// its slot with an unconfigured index.
pub fn install(app: &AppHandle) {
    let seams = with_app_answers(ListingSeams::new(Arc::new(events::TauriListingEvents::new(
        app.clone(),
    ))))
    // The app's own runtime, so a listing's background work shares one
    // thread pool with everything else, and a watcher's OS thread (no
    // reactor) can still start a refresh.
    .runtime(tauri::async_runtime::handle().inner().clone())
    // The portal's `.git/` landing rows. Everything below `.git/` is a
    // routed volume instead; this seam is what keeps the six rows out of
    // every walker.
    .overlay(crate::file_system::git::overlay::overlay())
    // FIRST: arms a listing's change watch on open and disarms it on close,
    // after the cache removal like every observer.
    .observer(crate::file_system::watcher::observer())
    // Keeps a repo's `.git/*` watcher armed for as long as a pane shows one
    // of its virtual listings, so a lone `branches/` pane stays live.
    .observer(crate::file_system::git::arming::observer())
    // Routes the index's folder-size updates to the listings they touch.
    .observer(crate::listing_index_sizes::observer());
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

/// The app's answers to the engine's questions: which volume, what the index
/// knows, the foreground lease, the reading language, and the mount table.
fn with_app_answers(seams: ListingSeams) -> ListingSeams {
    seams
        .volumes(Arc::new(volumes::AppVolumes))
        .index(Arc::new(index::AppListingIndex))
        .leases(Arc::new(leases::AppLeases))
        .locale(Arc::new(locale::AppLocale))
        .mounts(Arc::new(mounts::AppMountTable))
}

/// What the test binary's shared slot holds, since nothing there calls
/// [`install`]: detached events and runtime, every app answer (the tests list
/// through the real volume registry, index, lease, language, and mount table),
/// and the change watch, so a listing on a watchable volume arms a watch in
/// tests as it does in the app.
#[cfg(test)]
pub(crate) fn test_seams() -> ListingSeams {
    with_app_answers(ListingSeams::detached()).observer(crate::file_system::watcher::observer())
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
/// 3. The panes (`listing::caching::notify_directory_changed`).
///
/// The coupling is one-directional: listing → indexer, never the reverse.
pub(crate) fn directory_changed(volume_id: &str, parent_path: &Path, change: DirectoryChange) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::index_host::index().apply_directory_change(volume_id, parent_path, &change);

    #[cfg(target_os = "macos")]
    crate::file_system::sync_status::invalidate_dir(parent_path);

    crate::file_system::listing::caching::notify_directory_changed(volume_id, parent_path, change);
}
