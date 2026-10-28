//! The loop guard and the wrap-up: how a turn notices it's going in circles, and what each
//! per-answer limit records when it ends the tool loop.
//!
//! The constants and why they sit where they do: `context.rs` and `DETAILS.md` § Constants table.
//! The driver (`turn.rs`) asks this module once per call and once per round; nothing here runs a
//! call or touches the store.

use std::collections::HashMap;

use super::LOG_TARGET;
use super::events::AgentErrorKind;
use crate::agent::chat::context::{BACKGROUND_MAX_TOOL_TURNS, LOOP_GUARD_REPEATS, LOOP_GUARD_STALE_ROUNDS, WrapUp};
use crate::agent::llm::types::AgentToolCall;
use crate::agent::store::{TurnLimit, TurnLimitKind};

/// The loop guard: whether a turn is going in circles. Structural, never a judgement of content:
/// a call repeats when its identity (`AgentToolCall::identity_key`, the repeat breaker's and the
/// rail's "Same as above" key) was already sent this turn, and a round found something new when
/// any call in it went out for the first time and came back without a problem.
#[derive(Default)]
pub(super) struct LoopGuard {
    sent: HashMap<String, usize>,
    stale_rounds: usize,
}

/// What one round did, as the loop guard reads it.
#[derive(Default)]
pub(super) struct RoundJudgement {
    /// The most times any one call of the round has now been sent this turn.
    most_sent: usize,
    /// Some call went out for the first time and came back without a problem.
    found_new: bool,
}

impl RoundJudgement {
    pub(super) fn note(&mut self, sent: usize, ok: bool) {
        self.most_sent = self.most_sent.max(sent);
        self.found_new |= sent == 1 && ok;
    }
}

impl LoopGuard {
    /// Count one sending of `call`, answering how many times it has now been sent this turn.
    pub(super) fn sent(&mut self, call: &AgentToolCall) -> usize {
        let count = self.sent.entry(call.identity_key()).or_default();
        *count += 1;
        *count
    }

    /// After a round: wrap up once a call has been sent [`LOOP_GUARD_REPEATS`] times, or once
    /// [`LOOP_GUARD_STALE_ROUNDS`] rounds in a row found nothing new.
    pub(super) fn judge_round(&mut self, round: RoundJudgement) -> Option<WrapUp> {
        self.stale_rounds = if round.found_new { 0 } else { self.stale_rounds + 1 };
        if round.most_sent >= LOOP_GUARD_REPEATS {
            log::info!(target: LOG_TARGET, "loop guard: one tool call went out {} times; wrapping up", round.most_sent);
            return Some(WrapUp::RepeatedCall);
        }
        if self.stale_rounds >= LOOP_GUARD_STALE_ROUNDS {
            log::info!(target: LOG_TARGET, "loop guard: {} rounds in a row found nothing new; wrapping up", self.stale_rounds);
            return Some(WrapUp::NoProgress);
        }
        None
    }
}

/// The limit a wrap-up records, with the value that applied, before its call answers.
pub(super) fn wrap_up_limit(reason: WrapUp) -> TurnLimit {
    let (kind, max) = match reason {
        WrapUp::RoundCap => (TurnLimitKind::ToolRounds, BACKGROUND_MAX_TOOL_TURNS),
        WrapUp::RepeatedCall => (TurnLimitKind::RepeatedCall, LOOP_GUARD_REPEATS),
        WrapUp::NoProgress => (TurnLimitKind::NoProgress, LOOP_GUARD_STALE_ROUNDS),
    };
    TurnLimit {
        kind,
        max: u32::try_from(max).unwrap_or(u32::MAX),
        answered: false,
    }
}

/// How a wrap-up ends when its reply still reached for a tool: a repeat says it kept retrying the
/// same lookup, the rest that the answer hit its limit.
pub(super) fn unanswered_wrap_up(reason: WrapUp) -> AgentErrorKind {
    match reason {
        WrapUp::RepeatedCall => AgentErrorKind::RepeatedToolCall,
        WrapUp::RoundCap | WrapUp::NoProgress => AgentErrorKind::BudgetExhausted,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::agent::llm::types::ToolId;

    fn call(path: &str) -> AgentToolCall {
        AgentToolCall {
            call_id: "c".to_string(),
            tool: ToolId::ListDir,
            arguments: json!({ "path": path }),
            reasoning: None,
        }
    }

    /// One round of one call, judged: `ok` is whether it came back without a problem.
    fn round(guard: &mut LoopGuard, path: &str, ok: bool) -> Option<WrapUp> {
        let mut round = RoundJudgement::default();
        round.note(guard.sent(&call(path)), ok);
        guard.judge_round(round)
    }

    #[test]
    fn the_third_sending_of_one_call_wraps_up_even_when_it_succeeds() {
        let mut guard = LoopGuard::default();
        assert_eq!(round(&mut guard, "/a", true), None);
        assert_eq!(round(&mut guard, "/a", true), None);
        assert_eq!(round(&mut guard, "/a", true), Some(WrapUp::RepeatedCall));
    }

    #[test]
    fn rounds_that_find_nothing_new_wrap_up_and_one_that_does_resets_the_count() {
        let mut guard = LoopGuard::default();
        // Problems, each with different arguments: no repeat, but nothing found either.
        assert_eq!(round(&mut guard, "/x", false), None);
        assert_eq!(round(&mut guard, "/y", false), None);
        assert_eq!(round(&mut guard, "/ok", true), None, "a fresh answer resets the count");
        assert_eq!(round(&mut guard, "/z", false), None);
        assert_eq!(round(&mut guard, "/w", false), None);
        assert_eq!(round(&mut guard, "/v", false), Some(WrapUp::NoProgress));
    }

    /// Reading many different folders is progress however empty they are: a call is new by its
    /// arguments, never by what came back, so a run of empty listings never looks like a loop.
    #[test]
    fn many_different_successful_calls_never_trip_the_guard() {
        let mut guard = LoopGuard::default();
        for index in 0..100 {
            assert_eq!(round(&mut guard, &format!("/folder-{index}"), true), None);
        }
    }
}
