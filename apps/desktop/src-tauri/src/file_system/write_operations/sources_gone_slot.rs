//! Where a queued operation's sources-gone question stands.
//!
//! A leaf the operation state owns, like `conflict_slot.rs` and `outage_slot.rs`:
//! the state holds the slot and a cancel abandons it, while the queue check that
//! arms it and the answer that carries the operation on live in
//! `queued_sources.rs`, a layer above the state.

use std::sync::Mutex;

use tokio::sync::oneshot;

use super::types::WriteSourcesGoneEvent;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Where a queued operation's sources-gone question stands: asked (with the
/// sender the parked operation listens on), or not.
// DEFAULT-OK: the empty slot means "nothing asked", which is the truth for every
// operation until its queue check finds a source gone.
#[derive(Default)]
pub(crate) struct SourcesGoneSlot {
    asking: Mutex<Option<Asking>>,
}

struct Asking {
    carry_on: oneshot::Sender<()>,
    question: WriteSourcesGoneEvent,
}

impl SourcesGoneSlot {
    /// Arms the slot with `question` and hands back what the operation waits on.
    /// Arm BEFORE emitting: an answer can only follow a question someone saw.
    pub(super) fn arm(&self, question: WriteSourcesGoneEvent) -> oneshot::Receiver<()> {
        let (carry_on, answer) = oneshot::channel();
        *self.asking.lock_ignore_poison() = Some(Asking { carry_on, question });
        answer
    }

    /// The question this operation is parked on, for a reader that wasn't
    /// listening when it went out. `None` once answered or abandoned.
    pub(crate) fn pending(&self) -> Option<WriteSourcesGoneEvent> {
        self.asking
            .lock_ignore_poison()
            .as_ref()
            .map(|asking| asking.question.clone())
    }

    /// Drops the pending question, so the parked operation's receiver returns
    /// `Err` and the operation reads it as a cancel. What a cancel does.
    pub(crate) fn abandon(&self) {
        self.asking.lock_ignore_poison().take();
    }

    /// Delivers "carry on". `false` when nothing was asked, or the operation
    /// stopped listening.
    pub(super) fn carry_on(&self) -> bool {
        let Some(asking) = self.asking.lock_ignore_poison().take() else {
            return false;
        };
        asking.carry_on.send(()).is_ok()
    }
}
