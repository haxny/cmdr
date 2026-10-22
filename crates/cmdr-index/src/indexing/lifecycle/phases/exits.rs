//! Counts each volume's phase machines as their driver threads end. Test builds only.
//!
//! A stop cancels a machine without joining it, so its tail (the last walk's
//! release, a completion's collapse) can land after the volume restarted. A test
//! that pins what that tail may touch waits here for it to have run (#695).

use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;

/// One entry per machine that ended, named by its volume.
static EXITED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Called on the driver thread once the machine's run returns.
pub(super) fn record(volume_id: &str) {
    EXITED.lock_ignore_poison().push(volume_id.to_string());
}

/// How many of `volume_id`'s machines have ended in this process so far.
pub(crate) fn count(volume_id: &str) -> usize {
    EXITED.lock_ignore_poison().iter().filter(|id| *id == volume_id).count()
}
