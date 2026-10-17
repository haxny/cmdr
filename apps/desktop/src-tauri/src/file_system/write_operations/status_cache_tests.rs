//! Tests for the operation status cache and the busy-volume set it drives.
//!
//! Each test keys its cache entries per test (via `unique_id`) so they don't
//! collide when nextest runs them in one process against the module's
//! shared engine's status cache.

use super::*;
use crate::file_system::write_operations::manager::{OperationDescriptor, OperationSummaryText, PauseOutcome, manager};
use crate::file_system::write_operations::state::{OperationIntent, WriteOperationState, operation_states};
use crate::file_system::write_operations::test_support::{TestOperationGuard, placeholder_conflict};
use crate::file_system::write_operations::types::{
    LifecycleStatus, TransferWaitReason, WriteOperationPhase, WriteOperationType,
};
use cmdr_fs::volume::LaneKey;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::oneshot;

/// A cache key nothing else in the suite will collide with.
fn unique_id(label: &str) -> String {
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("test-status-cache-{label}-{n}-{:?}", std::thread::current().id())
}

/// Installs a live registry entry. The guard removes it on drop,
/// so a failing assertion can't leak it into another test.
fn install_state(label: &str, initial: OperationIntent) -> TestOperationGuard {
    let op = TestOperationGuard::register(label);
    op.state().intent().set_for_test(initial);
    op
}

// ---- register / update / unregister + list / get ----

#[test]
fn register_then_get_status_roundtrip() {
    let op = install_state("reg-get", OperationIntent::Running);
    let id = op.id().to_string();
    register_operation_status(&id, WriteOperationType::Copy, vec![]);

    let status = get_operation_status(&id).expect("operation should be in cache");
    assert_eq!(status.operation_id, id);
    assert_eq!(status.operation_type, WriteOperationType::Copy);
    assert_eq!(status.phase, WriteOperationPhase::Scanning);
    // This op was never registered with the MANAGER, only with the state map, so
    // there is no lifecycle record to report. That is the honest answer: the
    // status cache does not invent one.
    assert_eq!(status.lifecycle, None);
    assert_eq!(status.files_done, 0);
    assert_eq!(status.files_total, 0);
    assert_eq!(status.bytes_done, 0);
    assert_eq!(status.bytes_total, 0);

    drop(op);
    unregister_operation_status(&id);
    assert!(get_operation_status(&id).is_none());
}

/// A real managed operation on a lane nothing else touches, so admission runs it
/// immediately and the manager holds a `Running` record for it. Its status-cache
/// row is registered too, which is what `get_operation_status` reads.
///
/// Releases on drop so the lane frees for the rest of the process.
struct ManagedOperationFixture {
    id: String,
    lane: LaneKey,
    release: Option<oneshot::Sender<()>>,
}

impl ManagedOperationFixture {
    fn start(tag: &str) -> Self {
        let lane = LaneKey::new(format!("{}-lane", unique_id(tag)));
        Self::start_on_lane(tag, lane)
    }

    /// Same, on a caller-supplied lane, so a second operation can be made to
    /// queue behind the first.
    fn start_on_lane(tag: &str, lane: LaneKey) -> Self {
        let id = unique_id(tag);
        let (release_tx, release_rx) = oneshot::channel();
        let settle_id = id.clone();
        manager().spawn_managed(
            OperationDescriptor {
                operation_id: id.clone(),
                operation_type: WriteOperationType::Copy,
                lanes: vec![lane.clone()],
                volume_ids: vec![],
                summary: OperationSummaryText::default(),
                supports_rollback: false,
                preview_id: None,
                reverses: None,
            },
            Arc::new(WriteOperationState::new(Duration::from_millis(50))),
            Box::new(move |ops: Operations| {
                Box::pin(async move {
                    let _ = release_rx.await;
                    ops.manager().on_settled(&settle_id);
                })
            }),
        );
        register_operation_status(&id, WriteOperationType::Copy, vec![]);
        Self {
            id,
            lane,
            release: Some(release_tx),
        }
    }
}

impl Drop for ManagedOperationFixture {
    fn drop(&mut self) {
        unregister_operation_status(&self.id);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// The bug the `lifecycle` field exists to prevent: a paused operation keeps its
/// registry entry (it still holds its lane slots and parks between
/// files), so the presence test this replaced answered "running" while parked.
/// Every surface that steers off the answer — the pause/resume toggle above all
/// — then tried to pause what was already parked.
#[test]
fn a_paused_operation_reports_paused_not_running() {
    let op = ManagedOperationFixture::start("paused-lifecycle");
    assert_eq!(
        manager().lifecycle_status(&op.id),
        Some(LifecycleStatus::Running),
        "precondition: a fresh lane admits the operation immediately"
    );

    assert_eq!(manager().set_paused(&op.id, true), PauseOutcome::Applied);

    let status = get_operation_status(&op.id).expect("a paused operation is still in the status cache");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Paused));
    // The state-map entry is still there, which is exactly why the old boolean
    // got this backwards.
    assert!(operation_states().contains(&op.id));
}

/// The other half of the same bug, and the one that shows how far the boolean
/// missed: `spawn_managed` inserts the state entry BEFORE admission, so an
/// operation still waiting for its lane — one that has not written a byte and
/// may never be admitted — was reported as running too.
#[test]
fn a_queued_operation_reports_queued() {
    let holder = ManagedOperationFixture::start("queued-holder");
    // A second operation on the SAME lane can't be admitted until the first frees it.
    let queued = ManagedOperationFixture::start_on_lane("queued-waiter", holder.lane.clone());

    assert_eq!(manager().lifecycle_status(&queued.id), Some(LifecycleStatus::Queued));
    let status = get_operation_status(&queued.id).expect("a queued operation has a status-cache row");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Queued));
    assert!(
        operation_states().contains(&queued.id),
        "the state entry lands at spawn, not at admission — which is exactly why a presence test called this one running"
    );
}

#[test]
fn update_operation_status_overwrites_fields() {
    let id = unique_id("update");
    register_operation_status(&id, WriteOperationType::Move, vec![]);
    update_operation_status(
        &id,
        WriteOperationPhase::Copying,
        Some("a.txt".into()),
        3,
        10,
        500,
        1000,
    );
    let status = get_operation_status(&id).unwrap();
    assert_eq!(status.phase, WriteOperationPhase::Copying);
    assert_eq!(status.current_file.as_deref(), Some("a.txt"));
    assert_eq!(status.files_done, 3);
    assert_eq!(status.files_total, 10);
    assert_eq!(status.bytes_done, 500);
    assert_eq!(status.bytes_total, 1000);
    unregister_operation_status(&id);
}

// ---- activity on the snapshot (why isn't this moving?) ----

#[test]
fn a_parked_operation_names_its_wait_on_a_status_read() {
    // The sharp one: no `write-progress` event in flight, and none ever emitted.
    // A poller that missed the transient event — an agent reading `cmdr://state`
    // for the first time — still has to learn that this operation is parked on a
    // clash, and that it is a clash rather than a dead mount.
    let op = install_state("parked-conflict", OperationIntent::Running);
    let id = op.id().to_string();
    register_operation_status(&id, WriteOperationType::Copy, vec![]);

    // Nobody is being asked anything, and this operation keeps no in-flight
    // table, so it has nothing to claim. Absent, ❌ never a made-up `moving`.
    assert!(
        get_operation_status(&id).unwrap().activity.is_none(),
        "an operation with no probe and nobody being asked reports no activity"
    );

    let (tx, _rx) = oneshot::channel();
    op.state().conflict_slot.arm(tx, placeholder_conflict);

    let activity = get_operation_status(&id)
        .unwrap()
        .activity
        .expect("a parked operation answers for its own wait");
    assert_eq!(activity.waiting_on, TransferWaitReason::Conflict);
    assert_eq!(activity.in_flight, 0, "no in-flight table, so no honest count");
    assert_eq!(
        activity.still_for_seconds, 0,
        "a parked operation has been still for nobody's time but the answerer's"
    );

    unregister_operation_status(&id);
}

#[test]
fn a_paused_operation_reads_as_paused_rather_than_stalled() {
    let op = install_state("parked-pause", OperationIntent::Running);
    let id = op.id().to_string();
    register_operation_status(&id, WriteOperationType::Copy, vec![]);

    op.state().pause_gate.pause();

    let activity = get_operation_status(&id).unwrap().activity.expect("paused is a wait");
    assert_eq!(activity.waiting_on, TransferWaitReason::Paused);

    unregister_operation_status(&id);
}

#[test]
fn a_settled_operation_reports_no_activity() {
    // The status cache outlives registry, and once the state entry
    // is gone there is nothing left to classify.
    let op = install_state("settled", OperationIntent::Running);
    let id = op.id().to_string();
    register_operation_status(&id, WriteOperationType::Copy, vec![]);
    let (tx, _rx) = oneshot::channel();
    op.state().conflict_slot.arm(tx, placeholder_conflict);
    drop(op);

    let status = get_operation_status(&id).expect("the cache row outlives the state");
    assert!(status.activity.is_none(), "a settled operation waits on nothing");

    unregister_operation_status(&id);
}

#[test]
fn update_unknown_id_is_a_silent_noop() {
    // Pins the `&& get_mut` short-circuit. If `&&` becomes `||`, this would
    // dereference a None and panic.
    update_operation_status("no-such-op-xyzzy", WriteOperationPhase::Copying, None, 0, 0, 0, 0);
}

#[test]
fn indeterminate_compress_phases_clear_both_percent_fallback_axes() {
    let id = unique_id("compress-finishing");
    register_operation_status(&id, WriteOperationType::Compress, vec![]);
    for phase in [
        WriteOperationPhase::FinishingCompression,
        WriteOperationPhase::FinishingTransfer,
    ] {
        update_operation_status(&id, phase, None, 0, 0, 0, 0);
        let status = get_operation_status(&id).expect("operation status");
        assert_eq!((status.files_done, status.files_total), (0, 0));
        assert_eq!((status.bytes_done, status.bytes_total), (0, 0));
    }
    unregister_operation_status(&id);
}

// ---- `volumes-busy-changed` and the transfer gauge ----

/// `volumes-busy-changed` fires once per distinct change of the busy SET: a
/// progress update, a second op on an already-busy volume, a `root`-only op,
/// and releasing one of two holders all leave membership alone and stay quiet.
///
/// Its subject is the process-global dedupe (the busy set's last-reported value), which any
/// sibling's register moves, so it runs only one-test-per-process.
#[test]
fn volumes_busy_changed_fires_once_per_distinct_change() {
    if !crate::file_system::write_operations::test_support::one_test_per_process() {
        return;
    }
    let volume = unique_id("busy-dedupe-volume");
    let (first, second, local) = (unique_id("busy-1"), unique_id("busy-2"), unique_id("busy-root"));
    let emits = busy_emit_count();

    register_operation_status(&first, WriteOperationType::Copy, vec![volume.clone()]);
    assert_eq!(busy_emit_count(), emits + 1, "the volume became busy");
    update_operation_status(&first, WriteOperationPhase::Copying, None, 1, 2, 10, 20);
    register_operation_status(&second, WriteOperationType::Copy, vec![volume.clone()]);
    register_operation_status(&local, WriteOperationType::Copy, vec!["root".to_string()]);
    unregister_operation_status(&first);
    assert_eq!(busy_emit_count(), emits + 1, "nothing changed the set's membership");
    unregister_operation_status(&second);
    assert_eq!(busy_emit_count(), emits + 2, "the last holder left");
    unregister_operation_status(&local);
    assert_eq!(busy_emit_count(), emits + 2, "`root` was never in the set");
}

/// Reports the busy set like [`RecordingEngineEvents`], but holds the first
/// report inside the call until the test releases it, so a second update can
/// race it.
///
/// [`RecordingEngineEvents`]: super::super::host::testing::RecordingEngineEvents
struct GatedBusyEvents {
    reports: std::sync::Mutex<Vec<Vec<String>>>,
    first_entered: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    first_release: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl EngineEvents for GatedBusyEvents {
    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        let entered = self.first_entered.lock_ignore_poison().take();
        if let Some(entered) = entered {
            let release = self
                .first_release
                .lock_ignore_poison()
                .take()
                .expect("set with the sender");
            let _ = entered.send(());
            let _ = release.recv();
        }
        self.reports.lock_ignore_poison().push(volume_ids);
    }
    fn operations_changed(&self, _snapshot: crate::file_system::write_operations::types::OperationsChanged) {}
    fn transfer_started(&self, _volume_ids: &[String]) {}
    fn transfer_finished(&self, _volume_ids: &[String]) {}
    fn staging_leftovers_kept(&self, _notice: crate::file_system::write_operations::types::MoveLeftoversKeptEvent) {}
    fn instant_mutation_settled(&self, _fact: super::super::host::InstantMutationFact) {}
    fn error_report(&self, _report: super::super::host::OperationErrorReport) {}
    fn scan_wait_progress(&self, _event: crate::file_system::write_operations::types::WriteProgressEvent) {}
}

/// The last busy set reported is the current one, however two updates race.
/// Here the first register's report is held mid-call while a second register
/// runs: if the second could report around it, the first's older set (missing
/// the second volume) would land last, and the volume picker would offer Eject
/// on a drive a transfer just started on.
#[test]
fn racing_updates_report_the_current_busy_set_last() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let events = Arc::new(GatedBusyEvents {
        reports: std::sync::Mutex::new(Vec::new()),
        first_entered: std::sync::Mutex::new(Some(entered_tx)),
        first_release: std::sync::Mutex::new(Some(release_rx)),
    });
    let cache = Arc::new(StatusCache::new(Arc::clone(&events) as Arc<dyn EngineEvents>));

    let first = {
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || cache.register("first-op", WriteOperationType::Copy, vec!["usb-a".to_string()]))
    };
    entered_rx.recv().expect("the first register reaches its report");

    let (second_done_tx, second_done_rx) = std::sync::mpsc::channel();
    let second = {
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || {
            cache.register("second-op", WriteOperationType::Copy, vec!["usb-b".to_string()]);
            let _ = second_done_tx.send(());
        })
    };
    // A racy cache lets the second register finish while the first is held, so
    // wait for that; an atomic one keeps it waiting behind the first report,
    // and the timeout only moves the release along. Neither way can it turn a
    // correct cache red.
    let _ = second_done_rx.recv_timeout(Duration::from_millis(500));
    release_tx.send(()).expect("the first report is waiting");
    first.join().expect("first register");
    second.join().expect("second register");

    let reports = events.reports.lock_ignore_poison();
    assert_eq!(
        reports.last(),
        Some(&vec!["usb-a".to_string(), "usb-b".to_string()]),
        "the busy set reported last must be the current one, got {reports:?}"
    );
}

/// How a synthetic managed op ends.
#[derive(Debug, Clone, Copy)]
enum Ending {
    Completes,
    Cancels,
    Fails,
    HandlerPanics,
}

/// Every transfer-gauge start an op makes has exactly one finish, however the
/// op ends. A spare start held across the op proves the "exactly": a second
/// finish would spend it and clear the volume early.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_transfer_start_has_exactly_one_finish_however_the_op_ends() {
    use crate::file_system::write_operations::types::WriteOperationError;
    use crate::file_system::write_operations::{CollectorEventSink, OperationEventSink};
    use crate::priority::transfers::{note_transfer_finished, note_transfer_started, transfer_active};
    use std::sync::atomic::AtomicBool;

    for ending in [Ending::Completes, Ending::Cancels, Ending::Fails, Ending::HandlerPanics] {
        let volume = unique_id("gauge-volume");
        let spare = vec![volume.clone()];
        note_transfer_started(&spare);

        let events = Arc::new(CollectorEventSink::new());
        let active_inside = Arc::new(AtomicBool::new(false));
        let (seen, probe) = (Arc::clone(&active_inside), volume.clone());
        crate::file_system::write_operations::installed()
            .start_write_operation(
                Arc::clone(&events) as Arc<dyn OperationEventSink>,
                WriteOperationType::Copy,
                cmdr_operation_log::types::Initiator::User,
                200,
                vec![volume.clone()],
                vec![LaneKey::new(unique_id("gauge-lane"))],
                OperationSummaryText::default(),
                None,
                1,
                None,
                move |_events, _op_id, _state| {
                    seen.store(transfer_active(&probe), Ordering::SeqCst);
                    match ending {
                        Ending::Completes => Ok(()),
                        Ending::Cancels => Err(WriteOperationError::Cancelled {
                            message: "stopped".to_string(),
                        }),
                        Ending::Fails => Err(WriteOperationError::IoError {
                            path: "/x".to_string(),
                            message: "gone".to_string(),
                        }),
                        Ending::HandlerPanics => panic!("simulated handler panic"),
                    }
                },
            )
            .await
            .expect("start");
        crate::test_support::wait_until_async(Duration::from_secs(5), "the op to settle", || {
            !events.settled.lock_ignore_poison().is_empty()
        })
        .await;

        assert!(active_inside.load(Ordering::SeqCst), "{ending:?}: busy while it ran");
        assert!(
            transfer_active(&volume),
            "{ending:?}: the op's finish must not spend the spare start too"
        );
        note_transfer_finished(&spare);
        assert!(!transfer_active(&volume), "{ending:?}: its own start was finished");
    }
}

/// The panic net (`ManagedTaskGuard`, no `on_settled`) finishes the gauge too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panicking_op_task_finishes_its_transfer_start_exactly_once() {
    use crate::file_system::write_operations::manager::ManagedTaskGuard;
    use crate::priority::transfers::{note_transfer_finished, note_transfer_started, transfer_active};

    let volume = unique_id("gauge-panic-volume");
    let spare = vec![volume.clone()];
    note_transfer_started(&spare);

    let op = unique_id("gauge-panic-op");
    let id = op.clone();
    manager().spawn_managed(
        OperationDescriptor {
            operation_id: op.clone(),
            operation_type: WriteOperationType::Copy,
            lanes: vec![LaneKey::new(unique_id("gauge-panic-lane"))],
            volume_ids: vec![volume.clone()],
            summary: OperationSummaryText::default(),
            supports_rollback: false,
            preview_id: None,
            reverses: None,
        },
        Arc::new(WriteOperationState::new(Duration::from_millis(200))),
        Box::new(move |ops: Operations| {
            Box::pin(async move {
                let _guard = ManagedTaskGuard::new(ops.manager(), id);
                panic!("simulated task panic");
            })
        }),
    );
    crate::test_support::wait_until_async(Duration::from_secs(5), "the panic net to unregister the op", || {
        manager().lifecycle_status(&op).is_none() && get_operation_status(&op).is_none()
    })
    .await;

    assert!(transfer_active(&volume), "the net must not spend the spare start too");
    note_transfer_finished(&spare);
    // A wait, not an assert: the net lowers the gauge just after it drops the
    // status row the wait above watched.
    crate::test_support::wait_until_async(Duration::from_secs(5), "the net to finish the op's own start", || {
        !transfer_active(&volume)
    })
    .await;
}

/// A cache a panic poisoned still marks a transfer's volume busy, so Eject
/// stays refused mid-transfer. Each entry is a plain value: a panic can lose at
/// most the one update it interrupted, never tear the map.
#[test]
fn a_poisoned_cache_still_marks_a_running_transfers_volume_busy() {
    let cache = StatusCache::new(Arc::new(super::super::host::testing::RecordingEngineEvents::default()));
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = cache.entries.write_ignore_poison();
        panic!("a status update panicked, on purpose");
    }));
    assert!(poisoned.is_err() && cache.entries.is_poisoned());

    cache.register("poisoned-op", WriteOperationType::Copy, vec!["usb-stick".to_string()]);
    assert_eq!(
        cache.busy_volume_ids(),
        vec!["usb-stick".to_string()],
        "the volume a transfer touches is busy, poison or not"
    );
    cache.update("poisoned-op", WriteOperationPhase::Copying, None, 1, 2, 10, 20);
    assert_eq!(
        cache.snapshot("poisoned-op", None).map(|status| status.files_done),
        Some(1),
        "and its status still reads"
    );

    cache.unregister("poisoned-op");
    assert!(cache.busy_volume_ids().is_empty(), "until it's done");
}
