//! Reordering the line: moving a QUEUED operation earlier or later in the FIFO
//! `order` that admission walks. Running and paused operations hold lanes and
//! keep their place; only waiting ones trade places, and only with each other.
//!
//! Every test runs on a PRIVATE manager (`leaked_manager`): "the next queued
//! operation" is a question about the WHOLE line, so on the global singleton a
//! sibling test's queued op would sit between this test's rows and answer it.

use super::*;

/// The ids of `mgr`'s live rows, in snapshot (= admission) order.
fn order_of(mgr: &OperationManager) -> Vec<String> {
    mgr.list().into_iter().map(|row| row.operation_id).collect()
}

/// A held operation on `mgr`: registered, and released (settled) when the
/// returned sender fires. Whether it runs or queues depends on its lanes.
struct Held {
    id: String,
    started: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

fn hold(mgr: &'static OperationManager, label: &str, lanes: Vec<&str>) -> Held {
    let id = unique(label);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    mgr.spawn_managed(
        descriptor(&id, lanes),
        fresh_state(),
        gated_deferred_on(mgr, id.clone(), started_tx, release_rx),
    );
    Held {
        id,
        started: started_rx,
        release: release_tx,
    }
}

/// One running op holding `lane`, then `waiting` ops queued behind it on the
/// same lane, in registration order.
async fn line(mgr: &'static OperationManager, waiting: usize) -> (Held, Vec<Held>) {
    let lane = unique("lane");
    let mut running = hold(mgr, "running", vec![&lane]);
    (&mut running.started).await.expect("the first op is admitted at once");
    let queued = (0..waiting)
        .map(|i| hold(mgr, &format!("queued-{i}"), vec![&lane]))
        .collect::<Vec<_>>();
    for held in &queued {
        assert_eq!(mgr.lifecycle_status(&held.id), Some(LifecycleStatus::Queued));
    }
    (running, queued)
}

/// Drops whatever is still queued, then lets the running op settle, so no test
/// leaves a task parked on a oneshot.
fn finish(mgr: &'static OperationManager, running: Held, queued: Vec<Held>) {
    for held in &queued {
        mgr.cancel_if_queued(&held.id);
    }
    let _ = running.release.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_the_last_waiting_op_earlier_trades_places_with_the_one_before_it() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 2).await;
    let (b, c) = (queued[0].id.clone(), queued[1].id.clone());

    let outcome = mgr.move_queued(&c, QueueMoveDirection::Earlier);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), vec![running.id.clone(), c, b]);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_a_waiting_op_later_trades_places_with_the_one_after_it() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 3).await;
    let ids: Vec<String> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.move_queued(&ids[0], QueueMoveDirection::Later);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 2,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[1].clone(), ids[0].clone(), ids[2].clone()]
    );
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_and_last_waiting_ops_stay_put_at_their_edge_and_say_where_they_are() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 2).await;
    let before = order_of(mgr);
    let emits = mgr.emit_count();

    assert_eq!(
        mgr.move_queued(&queued[0].id, QueueMoveDirection::Earlier),
        QueueMoveOutcome::AtEdge {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        mgr.move_queued(&queued[1].id, QueueMoveDirection::Later),
        QueueMoveOutcome::AtEdge {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), before, "a move off the edge changes nothing");
    assert_eq!(
        mgr.emit_count(),
        emits,
        "and tells nobody about a change that didn't happen"
    );
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_or_unknown_op_does_not_move() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 1).await;
    let before = order_of(mgr);

    assert_eq!(
        mgr.move_queued(&running.id, QueueMoveDirection::Later),
        QueueMoveOutcome::NotQueued
    );
    assert_eq!(
        mgr.move_queued("no-such-op", QueueMoveDirection::Earlier),
        QueueMoveOutcome::NotQueued
    );
    assert_eq!(order_of(mgr), before);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn a_running_op_between_two_waiting_ones_keeps_its_place() {
    // [A running on L] [B waiting on L] [D running on M] [C waiting on L]:
    // C moving earlier passes D and trades with B, and D stays third.
    let mgr = leaked_manager();
    let lane = unique("lane");
    let other_lane = unique("other-lane");
    let mut a = hold(mgr, "a", vec![&lane]);
    (&mut a.started).await.expect("A admitted");
    let b = hold(mgr, "b", vec![&lane]);
    let mut d = hold(mgr, "d", vec![&other_lane]);
    (&mut d.started).await.expect("D admitted on its own lane");
    let c = hold(mgr, "c", vec![&lane]);

    let outcome = mgr.move_queued(&c.id, QueueMoveDirection::Earlier);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![a.id.clone(), c.id.clone(), d.id.clone(), b.id.clone()]
    );
    let _ = d.release.send(());
    finish(mgr, a, vec![b, c]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_tells_the_windows_once() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 2).await;
    let emits = mgr.emit_count();

    mgr.move_queued(&queued[1].id, QueueMoveDirection::Earlier);

    assert_eq!(mgr.emit_count(), emits + 1);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_op_moved_to_the_front_is_the_one_admitted_next() {
    // The point of the whole feature: the line decides who goes next.
    let mgr = leaked_manager();
    let (running, mut queued) = line(mgr, 2).await;
    let mut c = queued.pop().expect("two queued");
    let b = queued.pop().expect("two queued");

    mgr.move_queued(&c.id, QueueMoveDirection::Earlier);
    let _ = running.release.send(());

    tokio::time::timeout(WAIT, &mut c.started)
        .await
        .expect("the op moved to the front must be admitted when the lane frees")
        .expect("C started");
    assert_eq!(mgr.lifecycle_status(&b.id), Some(LifecycleStatus::Queued));

    mgr.cancel_if_queued(&b.id);
    let _ = c.release.send(());
}

// ── Placing a waiting op at a given place (a drag's drop) ──────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_the_last_waiting_op_first_shifts_the_others_back() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 3).await;
    let ids: Vec<String> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[2], 1);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[2].clone(), ids[0].clone(), ids[1].clone()]
    );
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_the_first_waiting_op_last_shifts_the_others_forward() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 3).await;
    let ids: Vec<String> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[0], 3);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 3,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[1].clone(), ids[2].clone(), ids[0].clone()]
    );
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_place_past_the_end_lands_last() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 2).await;
    let ids: Vec<String> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[0], 99);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), vec![running.id.clone(), ids[1].clone(), ids[0].clone()]);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_an_op_where_it_already_is_changes_nothing_and_tells_nobody() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 2).await;
    let before = order_of(mgr);
    let emits = mgr.emit_count();

    assert_eq!(
        mgr.place_queued(&queued[1].id, 2),
        QueueMoveOutcome::Unchanged {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), before);
    assert_eq!(mgr.emit_count(), emits);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_a_running_or_unknown_op_does_nothing() {
    let mgr = leaked_manager();
    let (running, queued) = line(mgr, 1).await;
    let before = order_of(mgr);

    assert_eq!(mgr.place_queued(&running.id, 1), QueueMoveOutcome::NotQueued);
    assert_eq!(mgr.place_queued("no-such-op", 1), QueueMoveOutcome::NotQueued);
    assert_eq!(order_of(mgr), before);
    finish(mgr, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn placing_an_op_first_passes_a_running_op_without_moving_it() {
    // [A running on L] [B waiting on L] [D running on M] [C waiting on L]:
    // C placed first takes B's slot, B takes C's, and D stays third.
    let mgr = leaked_manager();
    let lane = unique("lane");
    let other_lane = unique("other-lane");
    let mut a = hold(mgr, "a", vec![&lane]);
    (&mut a.started).await.expect("A admitted");
    let b = hold(mgr, "b", vec![&lane]);
    let mut d = hold(mgr, "d", vec![&other_lane]);
    (&mut d.started).await.expect("D admitted on its own lane");
    let c = hold(mgr, "c", vec![&lane]);

    let outcome = mgr.place_queued(&c.id, 1);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![a.id.clone(), c.id.clone(), d.id.clone(), b.id.clone()]
    );
    let _ = d.release.send(());
    finish(mgr, a, vec![b, c]);
}
