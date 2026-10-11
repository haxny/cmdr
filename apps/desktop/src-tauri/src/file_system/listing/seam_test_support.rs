//! Hooks the characterization pins use to watch the engine through the seams
//! issue #301 cuts: a marker overlay, an overlay that cancels a read at the
//! cache insert, a lifecycle observer that records what the cache and the
//! FSEvents watcher held when it was told, and a probe that records what the
//! cache held when a watch disarmed.
//!
//! The slot's overlay and observer lists are process-wide and keep the first
//! registration per id forever, so each hook registers ONCE and answers only
//! for what a test opted into: a volume NAME (overlays) or a listing id
//! (observer). Every other test's listings pass straight through.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, Once};

use tokio_util::sync::CancellationToken;

use super::cached_listing::LISTING_CACHE;
use super::host::testing::{register_observer, register_overlay};
use super::host::{ListingObserver, ListingOverlay, OpenedListing};
use super::listing_id::ListingId;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use cmdr_fs::volume::Volume;

/// The name a volume carries to have the marker overlay decorate it.
pub(crate) const MARKER_VOLUME_NAME: &str = "seam-pins marker volume";

/// The one row the marker overlay folds into every listing on a marker volume.
pub(crate) const MARKER_ROW: &str = "zz-marker-overlay-row";

/// Contributes [`MARKER_ROW`] to any directory on a volume named
/// [`MARKER_VOLUME_NAME`], and cancels the read of any volume a test armed with
/// [`cancel_at_decorate`].
struct SeamPinsOverlay;

/// Volume name → the read to cancel when the pipeline reaches the overlays.
static CANCEL_AT_DECORATE: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

impl ListingOverlay for SeamPinsOverlay {
    fn id(&self) -> &'static str {
        "seam-pins"
    }

    fn applies_to(&self, volume: &dyn Volume, _path: &Path) -> bool {
        if let Some(token) = CANCEL_AT_DECORATE.lock_ignore_poison().remove(volume.name()) {
            token.cancel();
        }
        volume.name() == MARKER_VOLUME_NAME
    }

    fn extra_entries(&self, _volume: &dyn Volume, path: &Path) -> Vec<FileEntry> {
        vec![FileEntry::new(
            MARKER_ROW.to_string(),
            path.join(MARKER_ROW).to_string_lossy().into_owned(),
            true,
            false,
        )]
    }
}

/// Registers the overlay hook, once per process.
pub(crate) fn ensure_seam_overlay() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| register_overlay(Arc::new(SeamPinsOverlay)));
}

/// Cancels `token` the moment a read of the volume named `volume_name` reaches
/// the overlay step: after the after-read cancel check, before the cache insert.
pub(crate) fn cancel_at_decorate(volume_name: &str, token: CancellationToken) {
    ensure_seam_overlay();
    CANCEL_AT_DECORATE
        .lock_ignore_poison()
        .insert(volume_name.to_string(), token);
}

/// One lifecycle call as the observer heard it, with whether the listing cache
/// held the listing at that moment (and, on a close, whether its FSEvents
/// watch was still armed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Heard {
    Opened { cached: bool },
    Closed { cached: bool, watching: bool },
}

/// Listing id → what the recording observer heard for it, in order.
static HEARD: LazyLock<Mutex<HashMap<String, Vec<Heard>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Listing ids a test opted into.
static WATCHED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

struct RecordingObserver;

impl RecordingObserver {
    fn hear(listing_id: &str, heard: impl FnOnce(bool) -> Heard) {
        if !WATCHED.lock_ignore_poison().contains(listing_id) {
            return;
        }
        let cached = LISTING_CACHE.read_ignore_poison().contains_key(listing_id);
        HEARD
            .lock_ignore_poison()
            .entry(listing_id.to_string())
            .or_default()
            .push(heard(cached));
    }
}

impl ListingObserver for RecordingObserver {
    fn id(&self) -> &'static str {
        "seam-pins-recorder"
    }

    fn listing_opened(&self, opened: &OpenedListing<'_>) {
        Self::hear(opened.id.as_str(), |cached| Heard::Opened { cached });
    }

    fn listing_closed(&self, id: &ListingId) {
        let watching = crate::file_system::watcher::WATCHER_MANAGER
            .read_ignore_poison()
            .watches
            .contains_key(id.as_str());
        Self::hear(id.as_str(), |cached| Heard::Closed { cached, watching });
    }
}

/// Makes the recording observer listen for `listing_id`.
pub(crate) fn record_lifecycle_of(listing_id: &str) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| register_observer(Arc::new(RecordingObserver)));
    WATCHED.lock_ignore_poison().insert(listing_id.to_string());
}

/// What the recording observer heard for `listing_id`, in order.
pub(crate) fn heard(listing_id: &str) -> Vec<Heard> {
    HEARD.lock_ignore_poison().get(listing_id).cloned().unwrap_or_default()
}

/// Listing ids whose watch disarms a test records, and per id whether the
/// listing cache still held the listing at each disarm.
static DISARMS: LazyLock<Mutex<HashMap<String, Vec<bool>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Makes the disarm probe record `listing_id`'s watch disarms.
#[cfg(target_os = "macos")]
pub(crate) fn record_disarms_of(listing_id: &str) {
    DISARMS.lock_ignore_poison().insert(listing_id.to_string(), Vec::new());
}

/// Called by `watcher::stop_watching`: notes whether the cache still held the
/// listing when its watch was disarmed, for listings a test opted into.
pub(crate) fn note_disarm(listing_id: &str) {
    if !DISARMS.lock_ignore_poison().contains_key(listing_id) {
        return;
    }
    let cached = LISTING_CACHE.read_ignore_poison().contains_key(listing_id);
    if let Some(seen) = DISARMS.lock_ignore_poison().get_mut(listing_id) {
        seen.push(cached);
    }
}

/// Per disarm of `listing_id`'s watch, whether the cache still held the listing.
#[cfg(target_os = "macos")]
pub(crate) fn disarms_of(listing_id: &str) -> Vec<bool> {
    DISARMS
        .lock_ignore_poison()
        .get(listing_id)
        .cloned()
        .unwrap_or_default()
}
