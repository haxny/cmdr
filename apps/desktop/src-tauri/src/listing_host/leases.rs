//! The priority policy, answering the listing engine's "the user is waiting on
//! this volume".

use crate::file_system::listing::host::{ForegroundHold, ForegroundLeases};

/// Holds `priority::foreground`'s lease on the volume, so a background upload or
/// index scan backs off for the whole of a listing the user waits on. That
/// module owns the signal.
pub(crate) struct AppLeases;

impl ForegroundLeases for AppLeases {
    fn hold(&self, volume_id: &str) -> ForegroundHold {
        ForegroundHold::new(crate::priority::foreground::lease_foreground_on(volume_id))
    }
}
