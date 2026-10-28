//! The spend pause: a turn the user started stops between rounds once its estimated spend passes
//! the user's threshold, and waits for "Keep going" or Stop.
//!
//! One [`PauseSlot`] per turn, created with the turn's cancel registration (`cancel.rs`), so the
//! `ask_cmdr_keep_going` command finds the running turn's slot by conversation the same way
//! `ask_cmdr_cancel` finds its token. A turn waits on its slot at the top of its loop, where no
//! model call or tool runs, so a paused turn spends nothing. Stop still ends it: the wait races
//! the turn's cancel token.
//!
//! **Why a oneshot per pause, not a `Notify`.** A `Notify` keeps a permit for a waiter that
//! hasn't arrived, so a double-clicked "Keep going" would quietly pre-approve the NEXT pause. A
//! fresh channel per pause answers exactly the question on screen.

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;

/// How a turn pauses on spend: past `threshold_micros` of estimated spend (and each further
/// multiple of it), it waits on `slot`. `None` on [`super::runtime::TurnParams`] means never,
/// which is what a background turn and the "Never" setting get.
#[derive(Clone)]
pub struct SpendPause {
    pub threshold_micros: i64,
    pub slot: Arc<PauseSlot>,
}

/// How a pause ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseOutcome {
    /// The user pressed Keep going.
    KeepGoing,
    /// The user pressed Stop (or Ask Cmdr was switched off), or the slot went away.
    Stopped,
}

/// One turn's pause point. Empty while the turn runs; holds the question while it waits.
#[derive(Default)]
pub struct PauseSlot {
    waiting: Mutex<Option<Waiting>>,
}

struct Waiting {
    cost_micros: i64,
    resume: oneshot::Sender<()>,
}

impl PauseSlot {
    /// Wait for the user's answer to "this answer has used about `cost_micros`, keep going?".
    /// The caller emits the event that shows the question; this only waits.
    pub async fn pause(&self, cost_micros: i64, cancel: &CancellationToken) -> PauseOutcome {
        let (resume, answered) = oneshot::channel();
        *self.waiting.lock_ignore_poison() = Some(Waiting { cost_micros, resume });
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => PauseOutcome::Stopped,
            answer = answered => match answer {
                Ok(()) => PauseOutcome::KeepGoing,
                Err(_) => PauseOutcome::Stopped,
            },
        };
        self.waiting.lock_ignore_poison().take();
        outcome
    }

    /// Answer the question on screen with Keep going. `false` when nothing was waiting (the turn
    /// already went on, stopped, or never paused), so a stale click resumes nothing.
    pub fn keep_going(&self) -> bool {
        let Some(waiting) = self.waiting.lock_ignore_poison().take() else {
            return false;
        };
        waiting.resume.send(()).is_ok()
    }

    /// The spend the turn is waiting on, or `None` while it isn't paused. A reloaded rail reads
    /// it to put the question back on screen.
    pub fn paused_at(&self) -> Option<i64> {
        self.waiting.lock_ignore_poison().as_ref().map(|w| w.cost_micros)
    }
}

/// The next spend the turn pauses at, after it reached `spent` with the pause set at `threshold`:
/// the smallest multiple of the threshold above what it has spent. So a turn that went on past
/// $2 asks again at $4, and one call that jumped from $1.90 to $4.10 asks once, at $6.
pub fn next_pause_at(spent: i64, threshold: i64) -> i64 {
    if threshold <= 0 {
        return i64::MAX;
    }
    (spent / threshold).saturating_add(1).saturating_mul(threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keep_going_answers_the_pause_on_screen_and_nothing_else() {
        let slot = Arc::new(PauseSlot::default());
        assert!(!slot.keep_going(), "nothing is waiting yet, so a click resumes nothing");

        let waiter = {
            let slot = slot.clone();
            tokio::spawn(async move { slot.pause(2_100_000, &CancellationToken::new()).await })
        };
        crate::test_support::wait_until_async(std::time::Duration::from_secs(2), "the turn to pause", || {
            slot.paused_at().is_some()
        })
        .await;
        assert_eq!(slot.paused_at(), Some(2_100_000));
        assert!(slot.keep_going());
        assert!(!slot.keep_going(), "a double click answers nothing more");
        assert_eq!(waiter.await.expect("join"), PauseOutcome::KeepGoing);
        assert_eq!(slot.paused_at(), None);
    }

    #[tokio::test]
    async fn stop_ends_a_pause() {
        let slot = PauseSlot::default();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(slot.pause(1, &cancel).await, PauseOutcome::Stopped);
        assert_eq!(slot.paused_at(), None, "a stopped pause leaves no question behind");
    }

    #[test]
    fn each_pause_waits_for_the_next_multiple_of_the_threshold() {
        assert_eq!(next_pause_at(0, 2_000_000), 2_000_000);
        assert_eq!(next_pause_at(2_100_000, 2_000_000), 4_000_000);
        assert_eq!(next_pause_at(4_100_000, 2_000_000), 6_000_000);
        assert_eq!(next_pause_at(5, 0), i64::MAX);
    }
}
