//! A landing rename an outage interrupted, settled once the destination is back
//! (`volume_outage.rs`).
//!
//! **The ambiguity.** A rename over a network share whose reply never arrives
//! may or may not have happened: the drop can land before the server saw the
//! request, or after it performed it. Asked again blind, the second case meets
//! our OWN file at the name, and a landing onto a name the caller believed free
//! refuses that as a clash nobody resolved (`staged_write.rs::land`): a copy that
//! waited out the outage then failed with `DestinationExists` over the file it
//! had just written. The live-SMB cell
//! `backend_suites/smb_outage_test.rs::smb_integration_a_copy_waits_out_a_dropped_network_and_finishes`
//! hit it whenever the cut fell inside a landing.
//!
//! **The proof it was ours.** The temp is a `.cmdr-tmp-<uuid>` name this
//! operation minted, and only two things ever take it away: our landing rename,
//! and our own discard. So once the destination is back, the landing counts as
//! done exactly when all three hold:
//!
//! - the temp is still in the operation's in-flight set, so no discard of ours
//!   removed it (`land`'s discard releases it the moment it does);
//! - the temp is gone from the destination, so something moved it, and the only
//!   move ever asked for it was ours, onto `final_path`;
//! - `final_path` holds a file of exactly the size we wrote, so nothing replaced
//!   it while the share was away.
//!
//! Anything short of that (a temp still there, a stat that fails, a size that
//! differs, a folder at the name) is NOT proof, and the file runs again the way
//! it did before: a temp still there means the rename never happened, and a
//! name someone else took meets the landing's ordinary clash refusal. ❌ Never
//! loosen the checks into "something is at the name": a file claimed by mistake
//! goes into the rollback ledger as ours, and an undo would delete it.

use std::path::Path;
use std::sync::Arc;

use super::super::in_flight_temps;
use super::super::state::WriteOperationState;
use super::recovered_name::discard_unplaced_temp;
use super::staged_write::dest_home;
use crate::file_system::volume::{Volume, VolumeError};

/// What a landing whose reply the outage swallowed turned out to have done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InterruptedLanding {
    /// The server performed it: our file is whole at its name. Count it done.
    Landed,
    /// It didn't happen, or nothing proves it did. The temp is cleared, and the
    /// file runs again from its first byte.
    RunAgain,
}

/// Settles a landing of `temp` onto `final_path` that failed on an outage with
/// nothing cleared (`FinalizeFailure::new_data_at: None`), now that the
/// destination is back. `bytes` is what the write put in the temp.
pub(super) async fn settle_interrupted_landing(
    state: &WriteOperationState,
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    final_path: &Path,
    bytes: u64,
) -> InterruptedLanding {
    if landed_before_the_drop(state, dest_volume, temp, final_path, bytes).await {
        in_flight_temps::deregister(state, temp, dest_home(state));
        return InterruptedLanding::Landed;
    }
    clear_unlanded_temp(state, dest_volume, temp).await;
    InterruptedLanding::RunAgain
}

/// The three checks in the module doc, cheapest first.
async fn landed_before_the_drop(
    state: &WriteOperationState,
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    final_path: &Path,
    bytes: u64,
) -> bool {
    if !in_flight_temps::is_registered(state, temp) {
        return false;
    }
    if !matches!(dest_volume.get_metadata(temp).await, Err(VolumeError::NotFound(_))) {
        return false;
    }
    match dest_volume.get_metadata(final_path).await {
        Ok(entry) => !entry.is_directory && !entry.is_symlink && entry.size == Some(bytes),
        Err(_) => false,
    }
}

/// Takes away the temp a failed landing left behind because the destination was
/// GONE when the landing tried to discard it, once the destination is back,
/// and drops it from the in-flight set. The file then runs again from a clean
/// slate.
///
/// ❗ Only for a landing that failed with nothing cleared (`new_data_at: None`):
/// then the temp is a spare copy and the destination still holds whatever it
/// held. With the way cleared, the temp is the only copy of the new bytes, and
/// `land` has already moved it out of temp space.
async fn clear_unlanded_temp(state: &WriteOperationState, dest_volume: &Arc<dyn Volume>, temp: &Path) {
    if discard_unplaced_temp(dest_volume, temp).await {
        in_flight_temps::deregister(state, temp, dest_home(state));
    }
}
