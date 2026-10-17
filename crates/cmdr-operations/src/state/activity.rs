//! What an operation is waiting on right now: the ONE classifier behind the
//! `activity` on every progress event and every `OperationStatus` snapshot.
//!
//! An operation with an in-flight table (a cross-volume transfer) is answered
//! by the transfer probe; one without answers for itself off the waits it can
//! name on its own: a pause, a conflict prompt, and a volume it's waiting on to
//! come back. Read in the same order the probe reads them, so the two can't
//! classify one wait two ways.

use super::{TransferActivity, TransferWaitReason, WriteOperationState, WriteProgressEvent};

impl WriteOperationState {
    /// What this operation is waiting on right now: the ONE classifier, for the
    /// progress event and the [`OperationStatus`](super::super::types::OperationStatus)
    /// snapshot alike.
    ///
    /// Both readers come through here so a poller and a subscriber can't
    /// disagree about a transfer's state; a snapshot saying `moving` while the
    /// event stream said `destination` sends an agent down the wrong branch.
    /// Classified at READ time, never cached: a wait that is over is worth
    /// nothing.
    ///
    /// Looked up by operation id rather than kept on the state, because the
    /// in-flight table lives in the transfer probe's registry. An operation that
    /// keeps no table (a local copy, a delete, a trash) misses that lookup and
    /// answers for itself off its own pause gate and conflict slot: the one
    /// thing it can still say is that somebody is being asked, and that is the
    /// one thing a view must not have to guess.
    ///
    /// `None` means "can't tell", ❌ never "it's moving".
    pub(crate) fn activity(
        &self,
        operation_id: &str,
        sending: Option<&WriteProgressEvent>,
    ) -> Option<TransferActivity> {
        if let Some(activity) = self.scope().stores().probes().activity_for(operation_id, sending) {
            return Some(activity);
        }
        self.decision_wait().map(|waiting_on| TransferActivity {
            // No table, so no honest count; and a parked operation has been
            // still for nobody's time but the answerer's.
            in_flight: 0,
            still_for_seconds: 0,
            waiting_on,
            opening_source: false,
            source_inbound_bytes_per_second: None,
            reconnecting_volume: self.reconnecting_volume(waiting_on),
        })
    }

    /// The volume named on an activity whose wait is [`TransferWaitReason::Reconnecting`],
    /// and `None` on every other: the name rides with the reason, never apart
    /// from it, so a view can't show "waiting for X" over a pause.
    pub(crate) fn reconnecting_volume(&self, waiting_on: TransferWaitReason) -> Option<String> {
        if waiting_on == TransferWaitReason::Reconnecting {
            self.outage.waiting_for()
        } else {
            None
        }
    }

    /// The wait an operation can name without an in-flight table: the two parks
    /// that hold for somebody's decision, and a volume it's waiting on to come
    /// back.
    ///
    /// All three are the ones the human-wait clock tracks
    /// ([`human_wait`](super::super::human_wait)), read in the same order
    /// `transfer_probe::wait_reason` reads them, so an operation with an
    /// in-flight table and one without classify the same wait the same way.
    fn decision_wait(&self) -> Option<TransferWaitReason> {
        if self.pause_gate.is_paused() {
            return Some(TransferWaitReason::Paused);
        }
        if self.conflict_slot.is_awaiting() {
            return Some(TransferWaitReason::Conflict);
        }
        if self.outage.waiting_for().is_some() {
            return Some(TransferWaitReason::Reconnecting);
        }
        None
    }
}
