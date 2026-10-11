//! Hooks the characterization pins use to watch the engine through the seams
//! issue #301 cuts: a marker overlay, an overlay that cancels a read at the
//! cache insert, and a lifecycle observer that records what the cache held when
//! it was told.
//!
//! The overlay and observer registries are process-wide and keep the first
//! registration per id forever, so each hook registers ONCE and answers only
//! for what a test opted into: a volume NAME (overlays) or a listing id
//! (observer). Every other test's listings pass straight through.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, Once};

use tokio_util::sync::CancellationToken;

use super::cached_listing::LISTING_CACHE;
use super::metadata::FileEntry;
use crate::file_system::volume::Volume;
use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};
use crate::listing_lifecycle::{ListingLifecycle, register_listing_lifecycle};
use crate::listing_overlays::{ListingOverlay, register_listing_overlay};

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
    ONCE.call_once(|| register_listing_overlay(Arc::new(SeamPinsOverlay)));
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
/// held the listing at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Heard {
    Opened { cached: bool },
    Closed { cached: bool },
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

impl ListingLifecycle for RecordingObserver {
    fn id(&self) -> &'static str {
        "seam-pins-recorder"
    }

    fn listing_opened(&self, listing_id: &str, _volume: &dyn Volume, _path: &Path) {
        Self::hear(listing_id, |cached| Heard::Opened { cached });
    }

    fn listing_closed(&self, listing_id: &str) {
        Self::hear(listing_id, |cached| Heard::Closed { cached });
    }
}

/// Makes the recording observer listen for `listing_id`.
pub(crate) fn record_lifecycle_of(listing_id: &str) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| register_listing_lifecycle(Arc::new(RecordingObserver)));
    WATCHED.lock_ignore_poison().insert(listing_id.to_string());
}

/// What the recording observer heard for `listing_id`, in order.
pub(crate) fn heard(listing_id: &str) -> Vec<Heard> {
    HEARD.lock_ignore_poison().get(listing_id).cloned().unwrap_or_default()
}
