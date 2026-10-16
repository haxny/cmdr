//! Where the registry meets the freshness state machine.
//!
//! The transition table itself lives in `lifecycle/freshness.rs`; this is the
//! wiring that finds a volume's freshness handle, applies an event to it, reports
//! the change, and bumps the coverage epoch a continuity break calls for.
//!
//! The two entry points are LOCK DISCIPLINE, not style: `apply_freshness_event`
//! looks the handle up in the registry, then delegates to
//! `signals::apply_freshness_event_on`, which takes the handle directly and never
//! touches the registry at all (so it lives in that leaf, outside `state`).

use cmdr_fs::ignore_poison::IgnorePoison;
use std::sync::Arc;

use super::{INDEX_REGISTRY, get_writer_and_flux_for};
use crate::indexing::lifecycle::freshness::{Freshness, FreshnessEvent};
use crate::indexing::lifecycle::signals::apply_freshness_event_on;
use crate::indexing::writer::WriteMessage;

/// Apply a freshness transition for a volume via the pure state machine
/// (`freshness::Freshness::on`). No-op if the volume has no registered instance
/// or no current freshness value yet.
///
/// EXTERNAL callers that only have a volume id (the live-watch layer:
/// `transports/smb/index` / `transports/mtp/index` firing `WatcherDied` / `OverflowUnrecoverable`)
/// use this entry point — it looks the instance's freshness `Arc` up UNDER the
/// registry lock, then delegates to `apply_freshness_event_on` (which does the
/// real transition + emit and never touches the registry).
///
/// ⚠️ Callers that ALREADY hold the registry lock (or can deadlock if it's
/// re-entered) must NOT use this. `IndexManager` fires its own scan-transition
/// events via `apply_freshness_event_on(&self.freshness, …)` using the `Arc` it
/// holds directly, so a `force_scan`/fallback caller can hold the registry lock
/// across `start_scan` without self-deadlocking on this re-lock.
pub(crate) fn apply_freshness_event(volume_id: &str, event: FreshnessEvent) {
    // Resolve the volume's freshness Arc and sink UNDER the registry lock, clone
    // them, then DROP the lock before the transition + report. The transition
    // itself never needs the registry, so holding it across the report is both
    // unnecessary and a re-entrancy hazard for any caller already under the lock.
    let Some((freshness, events)) = ({
        let reg = INDEX_REGISTRY.lock_ignore_poison();
        reg.get(volume_id).map(|instance| {
            (
                Arc::clone(&instance.signals.freshness),
                Arc::clone(&instance.signals.events),
            )
        })
    }) else {
        return;
    };
    apply_freshness_event_on(&freshness, events.as_ref(), volume_id, event);
}

/// Bump a volume's `current_epoch` on a continuity break that does NOT rescan
/// (watcher death, change-notify overflow, MTP disconnect, or the disconnect
/// completion branch). Routes through the volume's running writer so the bump
/// honors the single-writer-per-DB invariant. No-op for an unindexed or
/// not-yet-`Running` volume (a scan-start funnel bumps via its own flushed send,
/// not this helper).
///
/// Fire-and-forget: the bump rides the writer channel in order behind any
/// in-flight writes, so a subsequent read may briefly see the old epoch. That's
/// benign — the freshness badge already flips Stale alongside this call, and the
/// per-dir stale derivation self-corrects once the bump commits.
pub(crate) fn bump_current_epoch_for(volume_id: &str) {
    if let Some((writer, _scanning)) = get_writer_and_flux_for(volume_id)
        && let Err(e) = writer.send(WriteMessage::BumpCurrentEpoch)
    {
        log::warn!("bump_current_epoch_for('{volume_id}'): writer send failed: {e}");
    }
}

/// Read a volume's current freshness, if it has a registered instance.
pub(crate) fn get_freshness(volume_id: &str) -> Option<Freshness> {
    INDEX_REGISTRY
        .lock_ignore_poison()
        .get(volume_id)
        .and_then(|i| *i.signals.freshness.lock_ignore_poison())
}
