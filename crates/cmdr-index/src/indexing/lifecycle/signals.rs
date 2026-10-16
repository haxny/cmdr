//! A volume's shared signal handles, and the freshness transition fired on them.
//!
//! [`VolumeSignals`] is the value bag a volume's registry instance and its
//! `IndexManager` both hold; [`apply_freshness_event_on`] is the transition that
//! runs against those handles directly. Neither touches the registry, which is why
//! they live in this leaf rather than in `state`: the manager, the phase machine,
//! scan completion, and the SMB/MTP transports reach them without importing the
//! registry module. The registry-lookup wrapper, `state::apply_freshness_event`,
//! stays beside the registry it locks.

use cmdr_fs::ignore_poison::IgnorePoison;
use std::sync::Arc;

use super::freshness::{Freshness, FreshnessEvent};
use super::lifecycle_bus;

/// The two handles a volume's registry instance and its `IndexManager` both
/// hold: its freshness signal and where its reports go. Passed as ONE value so the
/// two can never be built with different halves — a manager firing freshness
/// through a different `Arc` is exactly the class of bug this shape rules out.
///
/// The third thing they share, the volume's stop signal with its hold, is a
/// [`VolumeWork`](crate::indexing::hold::VolumeWork) only the reservation mints
/// (`state/reservation.rs`).
#[derive(Clone)]
pub(crate) struct VolumeSignals {
    /// This volume's freshness signal (gray = absent instance; blue/green/yellow
    /// = the `Freshness` variants). `Arc<Mutex<…>>` so scan-transition tasks and
    /// the live-watch layer can flip it without holding the registry lock.
    /// `None` means "not yet determined" (e.g. mid-initialization before the
    /// first scan transition); a `Running` volume always carries `Some`. The
    /// state machine itself lives in `lifecycle/freshness.rs`. See DETAILS §
    /// "The freshness model".
    pub(crate) freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
    /// Where this volume's reports go. Held per volume rather than in a
    /// process-wide slot so the handle-free seams (the freshness transition, the
    /// failure supervisor) stay per-volume, like every other invariant here.
    pub(crate) events: Arc<dyn crate::EventSink>,
}

impl VolumeSignals {
    /// Bundle a volume's shared handles: the caller's freshness and sink.
    pub(crate) fn new(freshness: Arc<std::sync::Mutex<Option<Freshness>>>, events: Arc<dyn crate::EventSink>) -> Self {
        Self { freshness, events }
    }
}

/// The actual freshness transition + FE emit, operating on a volume's freshness
/// `Arc` DIRECTLY — it NEVER locks `INDEX_REGISTRY`. This is the lock-discipline
/// seam: `IndexManager` holds a clone of its volume's freshness `Arc` and fires
/// scan transitions through here, so a scan-start firing can't re-enter the
/// registry (the self-deadlock a held-registry caller like `force_scan` hit).
///
/// `state::apply_freshness_event` is the registry-lookup wrapper for external
/// callers that only have a volume id.
pub(crate) fn apply_freshness_event_on(
    freshness: &std::sync::Mutex<Option<Freshness>>,
    events: &dyn crate::EventSink,
    volume_id: &str,
    event: FreshnessEvent,
) {
    // `ScanStarted` is total even from "not yet determined": a scan can begin on
    // a volume that has no freshness yet (first ever scan). Seed it so the
    // transition is meaningful, then apply the event.
    //
    // We compute the next value under the freshness lock, then emit the FE event
    // AFTER dropping it (the report never needs it, and holding a std Mutex
    // across a host call risks contention). The event fires only on an actual value
    // change, so the FE's one-time stale dialog sees the exact Fresh→Stale
    // transition (subscribe-don't-poll).
    let changed_to = {
        let mut f = freshness.lock_ignore_poison();
        let previous = *f;
        let next = f.unwrap_or(Freshness::Scanning).on(event);
        *f = Some(next);
        (previous != Some(next)).then_some(next)
    };

    if let Some(next) = changed_to {
        events.emit(crate::IndexEvent::FreshnessChanged {
            volume_id: volume_id.to_string(),
            freshness: next,
        });
    }

    // Publish scan completion on the neutral in-process lifecycle bus, alongside
    // the frontend `.emit` above. A backend subsystem (the importance scheduler)
    // drives its full-volume recompute off this, without `indexing/` depending on
    // it (plan Decision 4). We fire on the EVENT, not on a freshness change: a
    // Fresh→Fresh rescan completion still means new data to rescore, and it must
    // notify the bus even though the badge didn't move.
    // An unwatched volume's completion is new data just the same, however its
    // badge reads.
    if matches!(
        event,
        FreshnessEvent::ScanCompleted | FreshnessEvent::ScanCompletedUnwatched
    ) {
        lifecycle_bus::publish_scan_completed(volume_id);
    }
}
