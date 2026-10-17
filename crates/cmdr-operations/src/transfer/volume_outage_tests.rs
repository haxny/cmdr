//! The outage wait as a state machine: a volume goes away, the operation waits
//! (named, cancelable, bounded), and the wait ends Back, GaveUp, Cancelled, or
//! NotWaitable. The end-to-end cells, through the real copy pipeline, are in
//! `volume/copy_outage_tests.rs`.

use super::*;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::types::TransferWaitReason;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A policy that runs a whole outage in well under a second.
const QUICK: OutagePolicy = OutagePolicy {
    budget: Duration::from_millis(400),
    cadence: &[Duration::from_millis(10), Duration::from_millis(20)],
    poll_every: Duration::from_millis(5),
    attempt_timeout: Duration::from_millis(50),
};

/// How long a test waits for something it caused to show.
const WAIT: Duration = Duration::from_secs(5);

fn quick_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(QUICK),
    )
}

fn nas(lane: &str) -> (Arc<OutageVolume>, Arc<dyn Volume>) {
    let volume = OutageVolume::wrap(Arc::new(InMemoryVolume::new("Naspolya")), lane);
    let dynamic: Arc<dyn Volume> = Arc::clone(&volume) as Arc<dyn Volume>;
    (volume, dynamic)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_comes_back_ends_the_wait() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-back", quick_state(&engine));
    let (nas, volume) = nas("outage-back");
    nas.go_away();
    let control = Arc::clone(&nas);
    tokio::spawn(async move {
        // Let a couple of attempts fail first, so the wait has really waited.
        wait_until_async(WAIT, "two failed attempts", || control.reconnect_attempts() >= 2).await;
        control.become_reachable();
    });

    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert_eq!(ended, OutageEnd::Back);
    assert!(nas.reconnect_attempts() >= 3, "it reconnected the volume itself");
    assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
    assert_eq!(
        op.state().outage.waiting_for(),
        None,
        "the wait is over, so nothing names it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_stays_gone_is_given_up_on_at_the_budget() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-gives-up", quick_state(&engine));
    let (nas, volume) = nas("outage-gives-up");
    nas.go_away();

    let started = std::time::Instant::now();
    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert_eq!(ended, OutageEnd::GaveUp);
    assert!(
        started.elapsed() >= QUICK.budget,
        "it waited the whole budget (took {:?})",
        started.elapsed()
    );
    assert!(nas.reconnect_attempts() > 3, "and kept trying throughout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_ends_the_wait_at_once() {
    let engine = TestOperations::new();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(60),
                ..QUICK
            }),
    );
    let op = engine.register_state("outage-cancel", state);
    let (nas, volume) = nas("outage-cancel");
    nas.go_away();
    let op_id = op.id().to_owned();
    let watched = Arc::clone(op.state());
    tokio::spawn(async move {
        wait_until_async(WAIT, "the wait to start", || watched.outage.waiting_for().is_some()).await;
        engine.cancel_write_operation(&op_id, false);
    });

    let started = std::time::Instant::now();
    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert_eq!(ended, OutageEnd::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a cancel can't wait out the budget (took {:?})",
        started.elapsed()
    );
}

/// One reconnect attempt that never returns (an SMB handshake the server
/// accepted and never answered) must not hold the wait: it's abandoned at the
/// attempt timeout, counted as "still down", and the next tick dials afresh
/// (`#406`). Before, the copy sat on the hung attempt until the budget ran out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_attempt_that_never_returns_is_abandoned_and_tried_again() {
    let engine = TestOperations::new();
    let policy = OutagePolicy {
        budget: Duration::from_secs(3),
        ..QUICK
    };
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(policy),
    );
    let op = engine.register_state("outage-hung-attempt", state);
    let (nas, volume) = nas("outage-hung-attempt");
    nas.go_away();
    nas.hang_reconnects();
    let control = Arc::clone(&nas);
    tokio::spawn(async move {
        // The first attempt hangs; a second one only starts if it was let go.
        wait_until_async(WAIT, "a second attempt after a hung one", || {
            control.reconnect_attempts() >= 2
        })
        .await;
        control.answer_reconnects();
        control.become_reachable();
    });

    let started = std::time::Instant::now();
    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert_eq!(ended, OutageEnd::Back, "the volume came back well within the budget");
    assert!(
        started.elapsed() < policy.budget,
        "a hung attempt can't hold the wait to its budget (took {:?})",
        started.elapsed()
    );
    assert!(
        nas.abandoned_reconnects() >= 1,
        "the hung attempt was dropped, not left running"
    );
}

/// A Cancel ends the wait while an attempt hangs, and drops that attempt
/// rather than leaving it dialing behind a finished operation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_drops_a_hung_reconnect_attempt() {
    let engine = TestOperations::new();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(60),
                attempt_timeout: Duration::from_secs(60),
                ..QUICK
            }),
    );
    let op = engine.register_state("outage-cancel-hung", state);
    let (nas, volume) = nas("outage-cancel-hung");
    nas.go_away();
    nas.hang_reconnects();
    let op_id = op.id().to_owned();
    let control = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "an attempt to hang", || control.reconnect_attempts() >= 1).await;
        engine.cancel_write_operation(&op_id, false);
    });

    let started = std::time::Instant::now();
    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert_eq!(ended, OutageEnd::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a cancel doesn't wait out a hung attempt (took {:?})",
        started.elapsed()
    );
    assert_eq!(
        nas.abandoned_reconnects(),
        nas.reconnect_attempts(),
        "every attempt the cancel caught hanging was dropped"
    );
}

/// The one look a job queued behind a give-up takes is bounded too: a hung
/// attempt there would otherwise hold the job until someone cancels it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_one_look_after_a_give_up_is_bounded_too() {
    let engine = TestOperations::new();
    let queued = engine.register_state("outage-hung-look-queued", quick_state(&engine));
    let first = engine.register_state("outage-hung-look-first", quick_state(&engine));
    let (nas, volume) = nas("outage-hung-look-lane");
    nas.go_away();
    assert_eq!(wait_while_out(first.state(), &[&volume], None).await, OutageEnd::GaveUp);

    nas.hang_reconnects();
    let started = std::time::Instant::now();
    let ended = tokio::time::timeout(WAIT, wait_while_out(queued.state(), &[&volume], None)).await;

    assert_eq!(
        ended,
        Ok(OutageEnd::GaveUp),
        "the hung look is abandoned, and the job gives up"
    );
    assert!(
        started.elapsed() < QUICK.budget,
        "at the attempt timeout, not later (took {:?})",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_needs_a_sign_in_is_not_waited_for() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-sign-in", quick_state(&engine));
    let (nas, volume) = nas("outage-sign-in");
    nas.go_away();
    nas.require_sign_in();

    // It reads `NeedsSignIn`, which time doesn't fix.
    assert_eq!(
        wait_while_out(op.state(), &[&volume], None).await,
        OutageEnd::NotWaitable
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_refused_for_its_credentials_stops_the_wait() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-auth", quick_state(&engine));
    let (nas, volume) = nas("outage-auth");
    // Disconnected, and the server refuses the saved password on the way back.
    nas.go_away();
    let refusing = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "a first attempt", || refusing.reconnect_attempts() >= 1).await;
        refusing.require_sign_in();
    });

    let ended = wait_while_out(op.state(), &[&volume], None).await;

    assert!(
        matches!(ended, OutageEnd::NotWaitable),
        "an auth refusal is a person's to fix, not the clock's: {ended:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_is_not_out_is_not_waited_for() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-none", quick_state(&engine));
    let (nas, volume) = nas("outage-none");
    let local: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Macintosh HD"));

    assert_eq!(
        wait_while_out(op.state(), &[&volume, &local], None).await,
        OutageEnd::NotWaitable
    );
    assert_eq!(nas.reconnect_attempts(), 0, "nothing was dialed");
}

/// Every window and an agent read the same thing while it waits: the reason,
/// and which volume. `still_for_seconds` stays 0, since a known wait isn't a stall.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_operation_names_the_volume_while_it_waits() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-named", quick_state(&engine));
    let (nas, volume) = nas("outage-named");
    nas.go_away();
    let state = Arc::clone(op.state());
    let op_id = op.id().to_owned();
    let control = Arc::clone(&nas);
    let observed = tokio::spawn(async move {
        wait_until_async(WAIT, "the wait to start", || state.outage.waiting_for().is_some()).await;
        let activity = state.activity(&op_id, None);
        control.become_reachable();
        activity
    });

    assert_eq!(wait_while_out(op.state(), &[&volume], None).await, OutageEnd::Back);
    let activity = observed
        .await
        .expect("the observer ran")
        .expect("a waiting op classifies itself");
    assert_eq!(activity.waiting_on, TransferWaitReason::Reconnecting);
    assert_eq!(activity.reconnecting_volume.as_deref(), Some("Naspolya"));
}

/// A pause outranks the wait in what the operation says, and stops the dialing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_wait_stops_dialing_and_resumes_with_a_fresh_budget() {
    let engine = TestOperations::new();
    let state = quick_state(&engine);
    let op = engine.register_state("outage-pause", Arc::clone(&state));
    let (nas, volume) = nas("outage-pause");
    nas.go_away();
    state.pause_gate.pause();

    let waiter_state = Arc::clone(&state);
    let waiter_volume = Arc::clone(&volume);
    let waiter = tokio::spawn(async move { wait_while_out(&waiter_state, &[&waiter_volume], None).await });

    wait_until_async(WAIT, "the wait to start", || state.outage.waiting_for().is_some()).await;
    // Longer than the whole budget: a paused wait must not give up.
    // allowed-test-sleep: the subject IS that time passing changes nothing while paused.
    tokio::time::sleep(QUICK.budget + Duration::from_millis(100)).await;
    assert!(!waiter.is_finished(), "a paused wait never times out");
    assert_eq!(nas.reconnect_attempts(), 0, "and dials nothing while paused");
    assert_eq!(
        state.activity(op.id(), None).map(|a| a.waiting_on),
        Some(TransferWaitReason::Paused),
        "the pause is what the operation says"
    );

    nas.become_reachable();
    state.pause_gate.resume();
    assert_eq!(waiter.await.expect("the wait ran"), OutageEnd::Back);
}

/// Jobs queued behind one that gave up don't each wait out the same outage
/// again: they're admitted one by one once it fails, and each would otherwise
/// add another fifteen minutes to a NAS that's already been waited for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_queued_before_a_give_up_does_not_wait_again() {
    let engine = TestOperations::new();
    let queued_state = quick_state(&engine);
    let first = engine.register_state("outage-first", quick_state(&engine));
    let queued = engine.register_state("outage-queued", Arc::clone(&queued_state));
    let (nas, volume) = nas("outage-queued-lane");
    nas.go_away();

    assert_eq!(wait_while_out(first.state(), &[&volume], None).await, OutageEnd::GaveUp);

    let attempts_before = nas.reconnect_attempts();
    let started = std::time::Instant::now();
    assert_eq!(
        wait_while_out(queued.state(), &[&volume], None).await,
        OutageEnd::GaveUp
    );
    assert!(
        started.elapsed() < QUICK.budget,
        "the queued job gives up at once (took {:?})",
        started.elapsed()
    );
    assert_eq!(
        nas.reconnect_attempts(),
        attempts_before + 1,
        "after one look, in case the volume came back meanwhile"
    );
}

/// A Retry is a new operation, created after the give-up: a fresh question that
/// gets a fresh wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_created_after_a_give_up_waits_afresh() {
    let engine = TestOperations::new();
    let first = engine.register_state("outage-before-retry", quick_state(&engine));
    let (nas, volume) = nas("outage-retry-lane");
    nas.go_away();
    assert_eq!(wait_while_out(first.state(), &[&volume], None).await, OutageEnd::GaveUp);

    let retry = engine.register_state("outage-retry", quick_state(&engine));
    let started = std::time::Instant::now();
    assert_eq!(wait_while_out(retry.state(), &[&volume], None).await, OutageEnd::GaveUp);
    assert!(
        started.elapsed() >= QUICK.budget,
        "the retry waited a whole budget of its own (took {:?})",
        started.elapsed()
    );
}

/// Several tasks of one operation waiting on one share share one deadline, so
/// they give up together rather than one by one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiters_on_one_volume_share_its_deadline() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-shared", quick_state(&engine));
    let (nas, volume) = nas("outage-shared");
    nas.go_away();

    let early_state = Arc::clone(op.state());
    let early_volume = Arc::clone(&volume);
    let early = tokio::spawn(async move { wait_while_out(&early_state, &[&early_volume], None).await });
    wait_until_async(WAIT, "the first waiter", || op.state().outage.waiting_for().is_some()).await;
    // allowed-test-sleep: the second waiter has to join measurably later than the first.
    tokio::time::sleep(QUICK.budget / 2).await;

    let started = std::time::Instant::now();
    assert_eq!(wait_while_out(op.state(), &[&volume], None).await, OutageEnd::GaveUp);
    assert!(
        started.elapsed() < QUICK.budget * 3 / 4,
        "the late waiter ends at the shared deadline, not a budget after it joined (took {:?})",
        started.elapsed()
    );
    assert_eq!(early.await.expect("the first waiter ran"), OutageEnd::GaveUp);
}

/// A reconnect somebody else made (a pane's own backoff) ends the wait too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reconnect_made_elsewhere_ends_the_wait() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-elsewhere", quick_state(&engine));
    let (nas, volume) = nas("outage-elsewhere");
    nas.go_away();
    let control = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "a first attempt", || control.reconnect_attempts() >= 1).await;
        control.reconnected_elsewhere();
    });

    assert_eq!(wait_while_out(op.state(), &[&volume], None).await, OutageEnd::Back);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn through_outages_runs_the_call_again_once_the_volume_is_back() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-through", quick_state(&engine));
    let (nas, volume) = nas("outage-through");
    let calls = AtomicUsize::new(0);
    let result = through_outages(op.state(), &[&volume], || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        let nas = Arc::clone(&nas);
        async move {
            if n == 0 {
                // The share drops under the first call, and is reachable again
                // by the time anyone dials it.
                nas.go_away();
                nas.become_reachable();
                return Err(VolumeError::DeviceDisconnected("session lost".into()));
            }
            Ok(n)
        }
    })
    .await;

    assert_eq!(result.expect("the second run lands"), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn through_outages_leaves_a_failure_that_is_not_an_outage_alone() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-not-transport", quick_state(&engine));
    let (nas, volume) = nas("outage-not-transport");
    let calls = AtomicUsize::new(0);
    let result: Result<(), VolumeError> = through_outages(op.state(), &[&volume], || {
        calls.fetch_add(1, Ordering::SeqCst);
        let nas = Arc::clone(&nas);
        async move {
            nas.go_away();
            Err(VolumeError::PermissionDenied {
                path: "/x".into(),
                raw_os_error: None,
            })
        }
    })
    .await;

    assert!(matches!(result, Err(VolumeError::PermissionDenied { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a refusal is reported, never re-run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn through_outages_stops_after_its_outage_cap() {
    let engine = TestOperations::new();
    let op = engine.register_state("outage-cap", quick_state(&engine));
    let (nas, volume) = nas("outage-cap");
    let calls = AtomicUsize::new(0);
    let result: Result<(), VolumeError> = through_outages(op.state(), &[&volume], || {
        calls.fetch_add(1, Ordering::SeqCst);
        let nas = Arc::clone(&nas);
        async move {
            // A link that flaps on every single call.
            nas.go_away();
            nas.become_reachable();
            Err(VolumeError::DeviceDisconnected("session lost".into()))
        }
    })
    .await;

    assert!(matches!(result, Err(VolumeError::DeviceDisconnected(_))));
    assert_eq!(
        calls.load(Ordering::SeqCst) as u32,
        MAX_OUTAGES_PER_ITEM + 1,
        "a flapping link is ridden out a bounded number of times"
    );
}
