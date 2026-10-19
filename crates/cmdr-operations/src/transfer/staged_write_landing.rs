//! How a staged write's completed temp takes its final name ([`land`]), and
//! what the in-flight ledgers hear as it does ([`LandingLedger`]). Split out
//! of `staged_write.rs`, which owns where a write goes; this owns how it
//! arrives.

use std::path::Path;
use std::sync::Arc;

use super::super::super::in_flight_temps::{self, TrackedRecord};
use super::super::super::state::WriteOperationState;
use super::super::recovered_name::{
    AfterARefusedDelete, FinalizeFailure, discard_unplaced_temp, rescue_out_of_temp_space, what_a_refused_delete_left,
};
use super::{LandingName, dest_home};
use cmdr_fs::volume::{Volume, VolumeError};

/// Moves a completed temp onto `final_path`.
///
/// Renames FIRST, and only clears `final_path` if that rename said something is
/// in the way. The conflict layer's `volume::finalize::finalize_safe_replace` is
/// the other way round because there the original is known to be in the way;
/// here it usually isn't (a fresh copy, or a conflict the resolver already
/// cleared), and a speculative delete would spend one extra round trip per file
/// on SMB and MTP for nothing. The name can still be taken — a `Rename`
/// resolution's `O_EXCL` placeholder, a cross-type Overwrite whose dest delete
/// failed, a racing writer — so the second attempt covers it.
///
/// ❗ **Only `AlreadyExists` earns the delete**, and this is the difference
/// between a transient blip and a destroyed file. A rename over a network
/// backend fails for plenty of reasons that say nothing about the destination:
/// the session blinked, the server refused, SFTP v3 collapsed an errno into its
/// one catch-all code. Clearing the way on every `Err` would delete a file the
/// user still has and then report the blip that "justified" it. It also covers a
/// backend that can delete but not rename, which would otherwise destroy the
/// destination and answer `NotSupported`.
///
/// ❗ **And only a name the CALLER claimed**, which is the other half of the
/// same question. `AlreadyExists` says something is in the way; only
/// [`LandingName`] says whose it is. Under
/// [`LandingName::ExpectedFree`] nothing resolved a conflict for this write, so
/// what's in the way is the user's file and a policy nobody applied to it: the
/// landing reports the clash and leaves both sides alone. A case-insensitive
/// destination is where that happens without any race — `Report.docx` renaming
/// onto a stored `report.docx` — and clearing the way there replaced files under
/// a Skip.
///
/// ❗ **A landing that fails with nothing cleared takes its temp away at once.**
/// The name still holds what it held, the source still holds the new bytes, and
/// a complete `.cmdr-tmp-*` of ours in the user's folder would otherwise wait
/// an hour for the stale-temp reap (`recovered_name::discard_unplaced_temp`).
/// That covers a rename refused for any reason but `AlreadyExists`, a clash on
/// a name the caller believed free, and a claimed name whose delete was refused
/// with the name still taken. ❗ Except a first rename that failed on the
/// transport ([`VolumeError::transport_fault`]): the server may have performed
/// it, so the temp stays registered and the caller settles which once the
/// destination is back (`interrupted_landing.rs`). If nothing settles it (the
/// wait gave up, or there was none to wait for), the operation's abandoned-write
/// cleanup removes it, as for any registered temp. When the way was CLEARED and the rename then
/// failed anyway, the temp is the only complete copy at the destination, so it
/// is never deleted: it leaves temp space instead ([`FinalizeFailure::new_data_at`]).
///
/// ❗ **Before it clears the way, the temp is re-recorded as a landing**
/// ([`LandingLedger::hold_for_landing`]): on disk as an `ItemKind::Landing`
/// at `size` (the byte count the write confirmed) before the delete goes out,
/// and out of the operation's in-flight set, so no in-operation sweep may
/// delete it and a crash leaves a record the sweep finishes the landing from.
/// The landing record retires when the rename lands it, when the rescue gives it a
/// real name, or when the refused delete left the name taken (the temp is a
/// spare copy then, and goes or turns back into a temp). A landing that ends
/// unsure, or is cut off, leaves it recorded for the sweep.
///
/// `ledger` hears every one of those moments. Until the first of them the temp
/// stays in the operation's in-flight set, so a landing that never finishes
/// with nothing cleared is still swept (see [`super::StagedWrite::commit`]).
pub(super) async fn land(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    final_path: &Path,
    landing: LandingName,
    size: u64,
    ledger: &LandingLedger<'_>,
) -> Result<(), FinalizeFailure> {
    let discard = || async {
        if discard_unplaced_temp(dest_volume.as_ref(), temp).await {
            ledger.release();
        }
    };
    let Err(first) = dest_volume.rename(temp, final_path, false).await else {
        ledger.release();
        return Ok(());
    };
    if !matches!(first, VolumeError::AlreadyExists(_)) {
        // ❗ A dropped link may have swallowed a rename the server performed, so
        // the temp stays registered for the caller's settle
        // (`interrupted_landing.rs`): its proof that the landing was ours is
        // that no discard of ours took the temp away. A discard here would run
        // on whatever session answers next (a sibling's fresh one, when the
        // error was the old session's) and, the temp being gone because it
        // landed, release it.
        if first.transport_fault().is_none() {
            discard().await;
        }
        return Err(first.into());
    }
    if matches!(landing, LandingName::ExpectedFree | LandingName::FreeInFreshFolder) {
        log::warn!(
            target: "copy",
            "staged write: {} is taken by something nobody resolved a conflict for; \
             leaving it alone and taking our temp {} away.",
            final_path.display(),
            temp.display(),
        );
        discard().await;
        return Err(first.into());
    }
    // From here the way may be cleared, and then the temp is the only complete
    // copy at the destination: it's recorded as a landing first, on disk, and
    // no sweep may touch it.
    let held = ledger.hold_for_landing(final_path, size).await;
    if let Err(refused) = dest_volume.delete(final_path).await
        && !matches!(refused, VolumeError::NotFound(_))
    {
        match what_a_refused_delete_left(dest_volume.as_ref(), final_path).await {
            AfterARefusedDelete::Gone => {}
            // Couldn't clear the way, so nothing was lost: the destination still
            // holds whatever was in the way, the temp is a spare copy, and the
            // rename error is the one to report.
            AfterARefusedDelete::StillThere => {
                if discard_unplaced_temp(dest_volume.as_ref(), temp).await {
                    ledger.retire_landing(held);
                } else {
                    ledger.landing_back_to_temp(held);
                }
                return Err(first.into());
            }
            // Maybe cleared, maybe not: the temp may be the only copy, so the
            // landing record stays for the sweep to finish or rescue.
            AfterARefusedDelete::Unknown => {
                ledger.keep_landing(held);
                return Err(first.into());
            }
        }
    }
    match dest_volume.rename(temp, final_path, false).await {
        Ok(()) => {
            ledger.retire_landing(held);
            Ok(())
        }
        // ❗ The way is cleared and the bytes couldn't take the name, so the
        // temp is now the only complete copy of a file with nothing at its
        // destination — and it wears a `.cmdr-tmp-*` name
        // `cleanup.rs::reap_stale_transfer_temps` matches once nothing records
        // it. Get it out of temp space and report where it went. Same act,
        // same reason, as `finalize::finalize_safe_replace`'s. A rescue that
        // couldn't rename either leaves the bytes on the temp, recorded.
        Err(error) => {
            let new_data_at = rescue_out_of_temp_space(dest_volume.as_ref(), temp, final_path).await;
            if new_data_at == temp {
                ledger.keep_landing(held);
            } else {
                ledger.retire_landing(held);
            }
            Err(FinalizeFailure {
                new_data_at: Some(new_data_at),
                error,
            })
        }
    }
}

/// One landing's temp in the in-flight ledgers, told each time the landing
/// changes what the temp is.
pub(super) struct LandingLedger<'a> {
    state: &'a WriteOperationState,
    temp: &'a Path,
    /// Set once the temp's `Temp` record is gone (released, or handed over as
    /// a landing), so however many of `land`'s exits get there, it retires
    /// once: each retirement appends to the persisted log.
    released: std::sync::atomic::AtomicBool,
}

impl<'a> LandingLedger<'a> {
    pub(super) fn new(state: &'a WriteOperationState, temp: &'a Path) -> Self {
        Self {
            state,
            temp,
            released: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The temp stopped being a removable copy of ours with nothing cleared:
    /// it landed, or it's gone.
    fn release(&self) {
        if !self.released.swap(true, std::sync::atomic::Ordering::Relaxed) {
            in_flight_temps::retire_temp(self.state, dest_home(self.state), self.temp);
        }
    }

    /// The way to `destination` is about to be cleared: the temp becomes a
    /// landing, on disk before this resolves (`in_flight_temps::hold_for_landing`).
    async fn hold_for_landing(&self, destination: &Path, expected_size: u64) -> Option<TrackedRecord> {
        self.released.store(true, std::sync::atomic::Ordering::Relaxed);
        in_flight_temps::hold_for_landing(self.state, dest_home(self.state), self.temp, destination, expected_size)
            .await
    }

    /// The landing is settled: the bytes have their name, a real name of their
    /// own, or are gone because they were a spare copy after all.
    fn retire_landing(&self, held: Option<TrackedRecord>) {
        if let Some(record) = held {
            in_flight_temps::retire(self.state, &record);
        }
    }

    /// The temp may be the destination's only copy and the landing couldn't
    /// finish: the record stays, for the sweep that next reaches the volume.
    fn keep_landing(&self, held: Option<TrackedRecord>) {
        if let Some(record) = held {
            in_flight_temps::keep_for_arrival(self.state, record);
        }
    }

    /// Nothing was cleared after all and the temp couldn't be taken away, so
    /// it's a removable copy of ours again: recorded as a temp (before the
    /// landing record goes) for the sweeps to remove.
    fn landing_back_to_temp(&self, held: Option<TrackedRecord>) {
        in_flight_temps::back_to_temp(self.state, dest_home(self.state), self.temp, held);
    }
}
