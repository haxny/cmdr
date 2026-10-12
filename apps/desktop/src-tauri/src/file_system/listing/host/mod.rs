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
//!   ([`ScratchVisibility`]); see `Listings::set_scratch_visibility`.
//! - The runtime: a [`tokio::runtime::Handle`], not a trait. ❌ Never
//!   `tokio::spawn` in the engine: it INHERITS an ambient runtime, and a watcher's
//!   OS thread has none, so it panics there. The engine spawns on the one it was
//!   built with.
//!
//! The host hands its answers to `Listings::builder` (`engine.rs`), and the
//! engine holds them as fields. ❌ A seam is called per read or per mutation,
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
pub mod volumes;

pub use events::{ListingEnd, ListingEvents, NoEvents};
pub use index::{ListingIndex, NoIndex};
pub use leases::{ForegroundHold, ForegroundLeases, NoLeases};
pub use locale::{NoLocale, ReadingLocale};
pub use mounts::{MountFacts, MountTable, NoMountTable};
pub use observers::{ListingObserver, OpenedListing};
pub use overlays::ListingOverlay;
pub use volumes::{ListingVolumes, NoVolumes, Unregistered};

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
