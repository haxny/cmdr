//! Where a move's "these items are Locked" question stands.
//!
//! A leaf the operation state owns, like `sources_gone_slot.rs`: the state holds
//! the slot and a cancel abandons it, while the move that arms it and the answer
//! that resumes it live in `locked_sources.rs`, a layer above the state. The
//! person holding the move up is on the `human_wait` clock for as long as it's
//! armed, so the ETA never charges their thinking to the copy.

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::human_wait::{HumanWaitClock, HumanWaitSource};
use super::types::{LockedSourcesAnswer, WriteLockedSourcesEvent};
use cmdr_fs::ignore_poison::IgnorePoison;

/// Where a move's Locked-items question stands: asked (with the sender the
/// parked move listens on), or not.
pub(crate) struct LockedSourcesSlot {
    asking: Mutex<Option<Asking>>,
    human_wait: Arc<HumanWaitClock>,
}

struct Asking {
    answer: oneshot::Sender<LockedSourcesAnswer>,
    question: WriteLockedSourcesEvent,
}

impl LockedSourcesSlot {
    pub(super) fn new(human_wait: Arc<HumanWaitClock>) -> Self {
        Self {
            asking: Mutex::new(None),
            human_wait,
        }
    }

    /// Arms the slot with `question` and hands back what the move waits on.
    /// Arm BEFORE emitting: an answer can only follow a question someone saw.
    pub(super) fn arm(&self, question: WriteLockedSourcesEvent) -> oneshot::Receiver<LockedSourcesAnswer> {
        let (answer, receiver) = oneshot::channel();
        let mut asking = self.asking.lock_ignore_poison();
        *asking = Some(Asking { answer, question });
        self.human_wait.set(HumanWaitSource::LockedQuestion, true);
        receiver
    }

    /// The question this move is parked on, for a reader that wasn't listening
    /// when it went out. `None` once answered or abandoned.
    pub(crate) fn pending(&self) -> Option<WriteLockedSourcesEvent> {
        self.asking
            .lock_ignore_poison()
            .as_ref()
            .map(|asking| asking.question.clone())
    }

    /// Drops the pending question, so the parked move's receiver returns `Err`
    /// and the move reads it as a cancel. What a cancel does.
    pub(crate) fn abandon(&self) {
        let mut asking = self.asking.lock_ignore_poison();
        asking.take();
        self.human_wait.set(HumanWaitSource::LockedQuestion, false);
    }

    /// Delivers the answer. `false` when nothing was asked, or the move stopped
    /// listening.
    pub(crate) fn answer(&self, answer: LockedSourcesAnswer) -> bool {
        let mut asking = self.asking.lock_ignore_poison();
        let Some(pending) = asking.take() else {
            return false;
        };
        self.human_wait.set(HumanWaitSource::LockedQuestion, false);
        pending.answer.send(answer).is_ok()
    }
}
