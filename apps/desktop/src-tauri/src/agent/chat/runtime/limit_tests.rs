//! The per-answer limits (`limits.rs`, `consume.rs`, `chat::spend_pause`): the background round
//! cap and its wrap-up call, no cap on a turn the user started, the loop guard, the stall check
//! in place of a total time limit, and the spend pause.
//!
//! Fixtures come from `test_support.rs`, shared with `tests.rs` and the other sibling suites.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::agent::chat::context::{
    BACKGROUND_MAX_TOOL_TURNS, LOOP_GUARD_REPEATS, LOOP_GUARD_STALE_ROUNDS, STREAM_STALL_TIMEOUT, WrapUp, wrap_up_note,
};
use crate::agent::chat::spend_pause::{PauseSlot, SpendPause};
use crate::test_support::wait_until_async;

/// `count` rounds of one tool call each, every call reading a different folder, so the loop
/// guard sees progress in each and only the limit under test can end the turn.
fn tool_rounds(count: usize) -> Vec<Program> {
    (0..count)
        .map(|index| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": format!("/folder-{index}") }))],
            usage: AgentUsage::default(),
        })
        .collect()
}

/// The params of a background turn: a wake, which nobody watches, so the round cap applies.
fn background_params(id: i64) -> TurnParams<'static> {
    let mut params = params(id, Some("tidy downloads"));
    params.origin = TurnOrigin::Wake;
    params
}

/// The stats the turn's one terminal event carried.
fn terminal_stats(events: &[AgentChatEvent]) -> Option<store::TurnStats> {
    events.iter().find_map(|event| match event {
        AgentChatEvent::Done { turn, .. } | AgentChatEvent::Failed { turn, .. } | AgentChatEvent::Stopped { turn } => {
            Some(*turn)
        }
        _ => None,
    })?
}

fn carries_wrap_up_note(message: &AgentMessage, reason: WrapUp) -> bool {
    message.role == AgentRole::User
        && message
            .parts
            .iter()
            .any(|part| matches!(part, AgentPart::Text(text) if *text == wrap_up_note(reason)))
}

/// Past the cap on tool rounds a BACKGROUND turn asks for an answer instead of dropping it: ONE
/// more call goes out carrying the wrap-up note, and its answer ends the turn. Regression anchor:
/// an eight-round Downloads cleanup (14 calls, 54 s, a tenth of its window used) ended on "hit its
/// limit" with nothing answered, because the cap cut the turn before the call that would have
/// read its last results.
#[tokio::test]
async fn past_the_round_cap_a_background_turn_makes_one_more_call_for_an_answer() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let mut programs = tool_rounds(BACKGROUND_MAX_TOOL_TURNS);
    programs.push(Program::Answer {
        chunks: vec!["Here's what I found so far.".to_string()],
        usage: AgentUsage::default(),
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &background_params(id),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let calls = llm.calls_seen();
    assert_eq!(
        calls.len(),
        BACKGROUND_MAX_TOOL_TURNS + 1,
        "one call past the cap, and only one"
    );
    let last = calls.last().expect("calls");
    assert!(
        last.last().is_some_and(|m| carries_wrap_up_note(m, WrapUp::RoundCap)),
        "the wrap-up call ends on the note"
    );
    assert!(
        calls[..BACKGROUND_MAX_TOOL_TURNS]
            .iter()
            .flatten()
            .all(|m| !carries_wrap_up_note(m, WrapUp::RoundCap)),
        "no call before the cap carries it"
    );
    let stored = store::list_messages(&conn, id, 1_000, 0).expect("list");
    let persisted_note = stored.iter().any(|row| match &row.content {
        store::StoredContent::Message { role, parts } => carries_wrap_up_note(
            &AgentMessage {
                role: *role,
                parts: parts.clone(),
                at: 0,
            },
            WrapUp::RoundCap,
        ),
        store::StoredContent::Event(_) => false,
    });
    assert!(
        !persisted_note,
        "the note is never persisted: it's this call's, not the thread's"
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::ToolRounds,
            max: BACKGROUND_MAX_TOOL_TURNS as u32,
            answered: true,
        }),
        "the stats name the limit and its value, so the rail can"
    );
}

/// A turn the user started has no round cap: they watch it, Stop ends it, and the loop guard and
/// the spend pause bound it. Regression anchor: at a cap of 25, a real cleanup that read its
/// destinations and proposed in batches stopped mid-job and asked the user to press Keep going.
#[tokio::test]
async fn a_turn_the_user_started_has_no_round_cap() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let rounds = BACKGROUND_MAX_TOOL_TURNS * 2;
    let mut programs = tool_rounds(rounds);
    programs.push(Program::Answer {
        chunks: vec!["All sorted.".to_string()],
        usage: AgentUsage::default(),
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("sort my downloads")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    assert_eq!(llm.calls_seen().len(), rounds + 1, "every round ran, then the answer");
    assert_eq!(terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit), None);
}

/// The wrap-up call is bounded too: a reply that still reaches for a tool ends the turn on the
/// limit, no tool runs, and no tool call is persisted without a result.
#[tokio::test]
async fn a_wrap_up_reply_that_still_calls_a_tool_ends_on_the_limit_without_running_it() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(tool_rounds(BACKGROUND_MAX_TOOL_TURNS + 4));
    let dispatcher = CountingOkDispatcher::default();
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &background_params(id),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(llm.calls_seen().len(), BACKGROUND_MAX_TOOL_TURNS + 1);
    assert_eq!(
        dispatcher.dispatched().len(),
        BACKGROUND_MAX_TOOL_TURNS,
        "the wrap-up's call never runs"
    );
    let stored = store::list_messages(&conn, id, 1_000, 0).expect("list");
    let last_message = stored
        .iter()
        .rev()
        .find_map(|row| match &row.content {
            store::StoredContent::Message { role, .. } => Some(*role),
            store::StoredContent::Event(_) => None,
        })
        .expect("a message");
    assert_eq!(
        last_message,
        AgentRole::Tool,
        "no dangling tool call for the next turn to load"
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::ToolRounds,
            max: BACKGROUND_MAX_TOOL_TURNS as u32,
            answered: false,
        })
    );
}

/// The loop guard's repeat half: the same call sent a third time, even one that SUCCEEDS each
/// time, wraps the turn up with an answer, and the stats say why.
#[tokio::test]
async fn the_same_call_sent_three_times_wraps_the_turn_up() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let mut programs: Vec<Program> = (0..3)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/photos" }))],
            usage: AgentUsage::default(),
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["Here's the folder.".to_string()],
        usage: AgentUsage::default(),
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what's in photos?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let calls = llm.calls_seen();
    assert_eq!(calls.len(), 4);
    assert!(
        calls[3]
            .last()
            .is_some_and(|m| carries_wrap_up_note(m, WrapUp::RepeatedCall))
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::RepeatedCall,
            max: LOOP_GUARD_REPEATS as u32,
            answered: true,
        })
    );
}

/// The loop guard's progress half: rounds that each try something different but find nothing (every
/// call comes back with a problem) wrap the turn up once there are three in a row.
#[tokio::test]
async fn rounds_that_find_nothing_new_wrap_the_turn_up() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let mut programs = tool_rounds(LOOP_GUARD_STALE_ROUNDS);
    programs.push(Program::Answer {
        chunks: vec!["I couldn't read those.".to_string()],
        usage: AgentUsage::default(),
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &FailingDispatcher::default(),
        &conn,
        &[],
        &params(id, Some("look around")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let calls = llm.calls_seen();
    assert_eq!(calls.len(), LOOP_GUARD_STALE_ROUNDS + 1);
    assert!(
        calls[LOOP_GUARD_STALE_ROUNDS]
            .last()
            .is_some_and(|m| carries_wrap_up_note(m, WrapUp::NoProgress))
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::NoProgress,
            max: LOOP_GUARD_STALE_ROUNDS as u32,
            answered: true,
        })
    );
}

/// What a turn spends across its rounds never ends it: only the context window bounds one call,
/// and the spend pause ASKS rather than ending anything. Regression anchor: a footer reading
/// 194,776 tokens beside a gauge at 2% looked like a token budget had fired.
#[tokio::test]
async fn tokens_summed_across_rounds_never_end_a_turn() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let heavy = AgentUsage {
        prompt_tokens: 900_000,
        completion_tokens: 20_000,
        ..AgentUsage::default()
    };
    let mut programs: Vec<Program> = (0..3)
        .map(|index| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": format!("/{index}") }))],
            usage: heavy,
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["done".to_string()],
        usage: heavy,
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("look around")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let stats = terminal_stats(&drain(&mut rx)).expect("stats");
    assert_eq!(stats.prompt_tokens, 3_600_000);
    assert_eq!(stats.limit, None);
}

/// A slow tool round is not a stall, and there is no total time limit any more: a turn whose
/// tool takes longer than the old ten-minute ceiling still answers.
#[tokio::test(start_paused = true)]
async fn a_long_turn_is_not_ended_by_its_length() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["slow but done".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &SleepingDispatcher { secs: 30 * 60 },
        &conn,
        &[],
        &params(id, Some("slow please")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
}

/// A provider that opens the stream and then sends nothing at all ends the call as stalled once
/// the stall window passes, and the stats name it so the rail can say what happened.
#[tokio::test(start_paused = true)]
async fn a_stream_that_goes_quiet_ends_as_stalled() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        },
        Program::Silent,
    ]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Timeout));
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::Stalled,
            max: STREAM_STALL_TIMEOUT.as_secs() as u32,
            answered: false,
        })
    );
}

/// A call that never even opens is bounded by the same window.
#[tokio::test(start_paused = true)]
async fn a_call_that_never_opens_ends_as_stalled() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::NeverOpens]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Timeout));
}

/// A model thinking quietly for longer than the stall window is fine while its provider keeps
/// pinging: each keep-alive restarts the window, so only true silence ends a call.
#[tokio::test(start_paused = true)]
async fn keep_alives_keep_a_quietly_thinking_model_alive() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let every = STREAM_STALL_TIMEOUT / 2;
    let llm = ProgrammableLlm::new(vec![Program::KeepAliveThenAnswer { beats: 10, every }]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("think hard")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(
        matches!(result, TurnResult::Answered { .. }),
        "five stall windows of thinking, never one of silence: {result:?}"
    );
}

// ── The spend pause ───────────────────────────────────────────────────────────

/// A priced cloud model, so each call's estimate is real: at Sonnet 5's $2 per million prompt
/// tokens, a 600,000-token round costs $1.20.
fn priced_params(id: i64, pause: Option<SpendPause>) -> TurnParams<'static> {
    let mut params = params(id, Some("clean up everything"));
    params.provider = ProviderTag::Anthropic;
    params.model = "claude-sonnet-5".to_string();
    params.spend_pause = pause;
    params
}

fn pricey_rounds(count: usize) -> Vec<Program> {
    let mut programs: Vec<Program> = (0..count)
        .map(|index| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": format!("/{index}") }))],
            usage: AgentUsage {
                prompt_tokens: 600_000,
                completion_tokens: 0,
                ..AgentUsage::default()
            },
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["done".to_string()],
        usage: AgentUsage::default(),
    });
    programs
}

fn two_dollar_pause() -> (SpendPause, Arc<PauseSlot>) {
    let slot = Arc::new(PauseSlot::default());
    (
        SpendPause {
            threshold_micros: 2_000_000,
            slot: slot.clone(),
        },
        slot,
    )
}

/// Past the threshold the turn stops between rounds and asks, spending nothing while it waits;
/// Keep going runs it on, and the next question waits for the next multiple of the threshold.
#[tokio::test]
async fn past_the_threshold_the_turn_pauses_and_keep_going_runs_it_on() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(pricey_rounds(3));
    let (pause, slot) = two_dollar_pause();
    let params = priced_params(id, Some(pause));
    let (tx, mut rx) = unbounded_channel();
    let cancel = CancellationToken::new();

    let turn = run_turn(&llm, &OkDispatcher, &conn, &[], &params, &tx, &cancel);
    let answer = async {
        wait_until_async(Duration::from_secs(2), "the turn to pause", || {
            slot.paused_at().is_some()
        })
        .await;
        assert_eq!(
            llm.calls_seen().len(),
            2,
            "paused after the round that passed $2, before the next call"
        );
        assert!(slot.keep_going());
    };
    let (result, ()) = tokio::join!(turn, answer);

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    assert_eq!(
        llm.calls_seen().len(),
        4,
        "every round ran once it was told to keep going ($3.60 asks nothing; $4 would)"
    );
    let events = drain(&mut rx);
    let pauses: Vec<i64> = events
        .iter()
        .filter_map(|event| match event {
            AgentChatEvent::SpendPaused { cost_micros } => Some(*cost_micros),
            _ => None,
        })
        .collect();
    assert_eq!(pauses, vec![2_400_000], "one question at $2.40; the next waits for $4");
    assert!(events.contains(&AgentChatEvent::SpendResumed));
}

/// Stop answers the question too: the turn ends stopped, and no further call goes out.
#[tokio::test]
async fn stop_while_paused_ends_the_turn_without_another_call() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(pricey_rounds(4));
    let (pause, slot) = two_dollar_pause();
    let params = priced_params(id, Some(pause));
    let (tx, _rx) = unbounded_channel();
    let cancel = CancellationToken::new();

    let turn = run_turn(&llm, &OkDispatcher, &conn, &[], &params, &tx, &cancel);
    let stop = async {
        wait_until_async(Duration::from_secs(2), "the turn to pause", || {
            slot.paused_at().is_some()
        })
        .await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(turn, stop);

    assert_eq!(result, TurnResult::Cancelled);
    assert_eq!(llm.calls_seen().len(), 2);
    assert_eq!(slot.paused_at(), None, "no question is left on screen");
}

/// "Never" (and every background turn) is `None`: the same spend runs straight through.
#[tokio::test]
async fn without_a_spend_pause_the_turn_never_asks() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(pricey_rounds(4));
    let params = priced_params(id, None);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(&llm, &OkDispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|event| matches!(event, AgentChatEvent::SpendPaused { .. }))
    );
}
