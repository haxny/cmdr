//! The in-memory volume's stand-in for a backend that keeps something warm
//! for a transfer reading from it (`Volume::hold_for_transfer`).

use super::InMemoryVolume;
use crate::volume::TransferHold;
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl InMemoryVolume {
    /// Makes [`Volume::hold_for_transfer`](crate::volume::Volume::hold_for_transfer) answer a hold that counts while it
    /// lives ([`Self::live_transfer_holds`]), standing in for a backend that
    /// keeps something warm for a transfer reading from it.
    pub fn counting_transfer_holds(mut self) -> Self {
        self.transfer_holds = Some(Arc::default());
        self
    }

    /// How many holds [`Volume::hold_for_transfer`](crate::volume::Volume::hold_for_transfer) handed out are alive.
    pub fn live_transfer_holds(&self) -> usize {
        self.transfer_holds
            .as_ref()
            .map_or(0, |live| live.load(Ordering::SeqCst))
    }

    /// What [`Volume::hold_for_transfer`](crate::volume::Volume::hold_for_transfer) answers: a counted hold when counting.
    pub(super) fn transfer_hold(&self) -> Option<TransferHold> {
        self.transfer_holds.as_ref().map(TransferHold::counted)
    }
}
