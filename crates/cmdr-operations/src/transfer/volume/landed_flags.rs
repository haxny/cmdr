//! The user flags a copied FILE puts on only after its landing rename: Locked
//! (and append-only) refuse that rename on local disk, and SMB's Read-only
//! makes a staged temp unremovable, so they can't ride the write the way the
//! rest of a file's metadata does (`Volume::write_from_stream` asks the stream
//! and puts it on the temp). The contract: `DETAILS.md` § "Copies keep the
//! source's metadata".
//!
//! The engine snapshots the set off the source stream before the write
//! consumes it ([`after_landing`]), a few bytes read off what the open already
//! holds, and spends one `Volume::apply_metadata` on the landed name only when
//! the set isn't empty ([`put_on`]): a plain file costs nothing, and so does a
//! Hidden-only file onto a destination whose write keeps Hidden itself (SMB's
//! `staged_flags`).
//!
//! Two landings. `stream_pipe_file` lands most files itself and puts the set on
//! right after. A safe-replace temp the CALLER lands (`WriteStaging::AlreadyStaged`)
//! is held here, keyed by that temp, until `finalize::finalize_safe_replace`
//! swaps it over the original and puts it on there ([`land_held`]).

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataClasses};

use super::super::super::state::WriteOperationState;
use cmdr_fs::ignore_poison::IgnorePoison as _;
use cmdr_fs::volume::{Volume, VolumeReadStream};

/// What the file `stream` reads waits to wear until it has its final name on
/// `dest`: empty unless `dest` keeps flags, `source` reports them, and the file
/// has one the write can't put on (`CarriedMetadata::after_landing`).
pub(super) fn after_landing(source: &dyn Volume, dest: &dyn Volume, stream: &dyn VolumeReadStream) -> CarriedMetadata {
    let caps = dest.metadata_caps();
    if !caps.stores.contains(MetadataClasses::FLAGS) || !source.metadata_caps().reports.contains(MetadataClasses::FLAGS)
    {
        return CarriedMetadata::default();
    }
    stream.metadata(MetadataClasses::FLAGS).after_landing(caps.staged_flags)
}

/// Puts `set` on the file that just landed at `landed`. Best effort: a refusal
/// is a `warn!` and a counted loss, never a failed copy.
pub(super) async fn put_on(dest: &Arc<dyn Volume>, landed: &Path, set: &CarriedMetadata, state: &WriteOperationState) {
    if set.is_empty() {
        return;
    }
    let report = dest.apply_metadata(landed, set, None).await;
    if report.flags == ClassOutcome::Refused {
        log::warn!(
            target: "transfer",
            "landed flags: {} landed without the source's flags {:?}: {report:?}",
            landed.display(),
            set.flags
        );
        state.metadata_losses.note_file(MetadataClasses::FLAGS);
    }
}

/// Holds `set` for the caller-landed temp `temp`, until
/// [`land_held`] knows its final name.
pub(super) fn hold(state: &WriteOperationState, temp: &Path, set: CarriedMetadata) {
    if set.is_empty() {
        return;
    }
    state
        .flags_after_landing
        .lock_ignore_poison()
        .insert(temp.to_path_buf(), set);
}

/// Takes what [`hold`] kept for `temp` and, when `landed_at` says the swap
/// happened, puts it on there. Taken either way, so a failed landing leaves no
/// entry behind.
pub(super) async fn land_held(
    dest: &Arc<dyn Volume>,
    temp: &Path,
    landed_at: Option<&Path>,
    state: &WriteOperationState,
) {
    let held = state.flags_after_landing.lock_ignore_poison().remove(temp);
    if let (Some(set), Some(landed)) = (held, landed_at) {
        put_on(dest, landed, &set, state).await;
    }
}
