//! A staged temp's kind change on its way to a name that may be cleared:
//! [`hold_for_landing`] makes it a landing, and [`back_to_temp`] undoes that.
//! A `#[path]` child of `in_flight_temps.rs`, whose ledgers it writes.

use std::path::Path;

use super::super::state::WriteOperationState;
use super::{ItemKind, Ledger, OnDisk, RecordHome, TrackedRecord, track_in, track_temp, tracked_record};
use cmdr_fs::ignore_poison::IgnorePoison;

/// The temp [`track_temp`] recorded holds complete bytes, and the way to
/// `destination` is about to be cleared: from here it may be the destination's
/// only copy. Re-records it as an [`ItemKind::Landing`], written before this
/// resolves, THEN retires its `Temp` record and drops it from the operation's
/// in-memory ledger, so no in-operation sweep may delete it.
///
/// ❗ That order is the guarantee. A crash between the two lines leaves both,
/// and the sweep settles a path recorded under both by the landing
/// (`in_flight_sweep.rs::without_superseded_temps`); the other order has a
/// moment where only the `Temp` record, which the sweep deletes, is on disk.
///
/// Hands back the landing's record for the caller to settle
/// ([`retire`](super::retire) once it lands or is rescued,
/// [`keep_for_arrival`](super::keep_for_arrival) while the destination can't
/// say). `Ok(None)` for a `home` of `None` on a ledger that keeps nothing on
/// disk anyway.
///
/// ❗ [`NotOnDisk`] when the ledger keeps a log and the landing record isn't in
/// it ([`OnDisk::Missing`], or a `home` of `None`, which no log can state): the
/// temp stays a temp, and the caller must ❌ not clear the way. A crash after
/// the clearing delete would leave the only copy of the new bytes on a temp no
/// record protects, which the next transfer into that folder reaps on sight
/// (its dates are the source's, so it reads as old at once).
pub(crate) async fn hold_for_landing(
    state: &WriteOperationState,
    home: Option<RecordHome>,
    temp: &Path,
    destination: &Path,
    expected_size: u64,
) -> Result<Option<TrackedRecord>, NotOnDisk> {
    let Some(home) = home else {
        if Ledger::of(state).store().keeps_a_log {
            return Err(NotOnDisk);
        }
        state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
        return Ok(None);
    };
    let temp_record = tracked_record(&home, ItemKind::Temp, temp);
    let (landing, on_disk) = track_in(
        state,
        home,
        ItemKind::Landing {
            destination: destination.to_path_buf(),
            expected_size,
        },
        temp,
    )
    .await;
    if on_disk == OnDisk::Missing {
        Ledger::of(state).retire_record(&landing.record);
        return Err(NotOnDisk);
    }
    state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    Ledger::of(state).retire_record(&temp_record);
    Ok(Some(landing))
}

/// A landing record that didn't reach the ledger's log ([`hold_for_landing`]).
#[derive(Debug)]
pub(crate) struct NotOnDisk;

/// The undo of [`hold_for_landing`], for a landing that cleared nothing and
/// couldn't take its temp away: the temp is a removable copy of ours again, so
/// it's tracked as one (in memory too) BEFORE the landing record retires.
pub(crate) fn back_to_temp(
    state: &WriteOperationState,
    home: Option<RecordHome>,
    temp: &Path,
    landing: Option<TrackedRecord>,
) {
    track_temp(state, home, temp);
    if let Some(landing) = landing {
        Ledger::of(state).retire_record(&landing.record);
    }
}
