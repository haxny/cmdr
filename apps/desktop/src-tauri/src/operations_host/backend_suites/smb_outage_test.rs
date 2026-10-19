//! A copy onto a real SMB share whose network goes away mid-copy waits for it
//! to come back and finishes (`#398`).
//!
//! The outage is a proxy of the test's own between the share and the Docker
//! fixture (`outage_proxy_test_support.rs`), so the shared container stack is
//! never touched. The wait itself is pinned without a server in
//! `transfer/volume_outage_tests.rs`; this cell is the part only a live share
//! can answer: that the real backend reads `Disconnected` when its socket
//! closes, refuses to reconnect while the network is gone, and reconnects once
//! it's back, so the copy lands every byte.
//!
//! A Cancel during the outage has a cell here too: it's reported at once, and
//! the temp it left on the share goes once the network is back
//! (`transfer/leftover_cleanup.rs`, pinned without a server in
//! `transfer/volume/copy_outage_tests.rs`).
//!
//! `#[ignore]`d like every `smb_integration_` test; the
//! `desktop-rust-integration-tests` check boots the Docker Samba stack and runs
//! it. Locally: `./apps/desktop/test/smb-servers/start.sh`, then
//! `cargo nextest run smb_outage_test --run-ignored all`.

use super::outage_proxy_test_support::OutageProxy;
use super::smb_test_support::*;

use crate::test_support::wait_until_async;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::ConnectionState;
use cmdr_operations::testing::new_operation_state;
use cmdr_operations::testing::{
    CleanupPolicy, CollectorEventSink, OutagePolicy, TestOperations, copy_volumes_with_progress,
};
use cmdr_operations::{
    ConflictInfo, ConflictResolution, DryRunResult, OperationEventSink, ScanProgressEvent, VolumeCopyConfig,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationPhase,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_smb::volume::SmbVolume;
use std::time::Instant;

/// Files in the folder. Enough that the outage lands in the middle, with files
/// on both sides of it: more than [`PAUSE_AFTER_FILES`] plus a whole concurrent
/// window (SMB's default is 10 slots), so the walk still has files it hasn't
/// started when the network goes. The walk parks per entry while paused, but a
/// file already in flight runs to its end, and with 12 files the whole rest of
/// the folder sometimes fit in the window and landed before the cut.
const FILE_COUNT: usize = 32;

/// Each file's size. Small enough that a file can go out as ONE compound write
/// at its final name (single-shot, no temp) and big enough that one can take the
/// staged writer instead, depending on the uplink smb2 has measured by then: a
/// drop can swallow the reply of either, and the copy has to settle both
/// (`transfer/interrupted_landing.rs`).
const FILE_BYTES: usize = 256 * 1024;

/// The copy pauses once this many files have landed, which is where the test
/// takes the network away.
const PAUSE_AFTER_FILES: usize = 3;

/// A bound on every wait in here, so a regression fails rather than hangs.
const WAIT: Duration = Duration::from_secs(30);

/// Reconnect quickly and often: what's under test is that the copy waits and
/// then carries on, not the production cadence.
const CADENCE: &[Duration] = &[Duration::from_millis(100), Duration::from_millis(200)];

fn content(index: usize) -> Vec<u8> {
    let seed = format!("cmdr-398-outage-{index}");
    let block = *blake3::hash(seed.as_bytes()).as_bytes();
    block.iter().copied().cycle().take(FILE_BYTES).collect()
}

/// Forwards everything to a collector, and shows `tap` every progress event
/// first: the copy's own emit decides the moment a cell acts (pauses the copy,
/// cuts the network), so the copy can't finish before the cell gets to.
struct ProgressTap {
    inner: Arc<CollectorEventSink>,
    tap: Box<dyn Fn(&WriteProgressEvent) + Send + Sync>,
}

impl OperationEventSink for ProgressTap {
    fn emit_progress(&self, event: WriteProgressEvent) {
        (self.tap)(&event);
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner.emit_complete(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.inner.emit_cancelled(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner.emit_error(event);
    }
    fn emit_conflict(&self, event: cmdr_operations::ArmedConflict) {
        self.inner.emit_conflict(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.inner.emit_settled(event);
    }
}

/// THE scenario from `#398`, against a real share: a folder copy loses the
/// network part-way, waits (named, not failed), and finishes once the network is
/// back, with every file byte-exact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_waits_out_a_dropped_network_and_finishes() {
    copy_through_a_cut(ConflictResolution::Stop).await;
}

/// The incident's `.DS_Store` shape: every file the copy writes REPLACES one
/// already on the share, so each lands by a swap (delete the original, rename
/// the new bytes onto its name), and the cut can fall inside one. A swap the
/// drop interrupted settles once the network is back, and every original ends
/// up replaced, with nothing parked under ` (recovered)` and no temp left.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_overwrite_waits_out_a_dropped_network_and_replaces_every_original() {
    copy_through_a_cut(ConflictResolution::Overwrite).await;
}

/// What the share holds at `index` before an Overwrite: a different size and
/// different bytes, so a file the copy left alone shows.
fn original(index: usize) -> Vec<u8> {
    let mut bytes = content(index);
    bytes.truncate(FILE_BYTES / 2);
    bytes.reverse();
    bytes
}

/// Copies the folder onto the share through a proxy cut part-way, and checks
/// the copy waited and every file landed byte-exact. Under
/// `ConflictResolution::Overwrite` the share starts with an [`original`] at
/// every name, so every file lands over one.
async fn copy_through_a_cut(conflict_resolution: ConflictResolution) {
    // A direct connection seeds and verifies; the copy runs through the proxy.
    let direct = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");
    if conflict_resolution == ConflictResolution::Overwrite {
        let seeded = PathBuf::from(&base).join("batch");
        direct.create_directory(&seeded).await.expect("the seeded folder");
        for index in 0..FILE_COUNT {
            direct
                .create_file(&seeded.join(format!("f-{index:02}.bin")), &original(index))
                .await
                .expect("an original on the share");
        }
    }

    let proxy = OutageProxy::start(guest_port()).await;
    let through_proxy: Arc<dyn Volume> =
        Arc::new(make_docker_volume_through(proxy.port(), crate::volume_host::host()).await);

    let local = tempfile::TempDir::new().expect("a local source dir");
    let folder = local.path().join("batch");
    std::fs::create_dir(&folder).expect("the source folder");
    for index in 0..FILE_COUNT {
        std::fs::write(folder.join(format!("f-{index:02}.bin")), content(index)).expect("a source file");
    }
    let source: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    // An engine of the cell's own: the copy runs on a state registered there.
    let engine = TestOperations::new();
    let state = Arc::new(
        new_operation_state(&engine, Duration::from_millis(50)).with_outage_policy(OutagePolicy {
            budget: Duration::from_secs(60),
            cadence: CADENCE,
            poll_every: Duration::from_millis(20),
            // Well inside `WAIT`, so one handshake that never returns can't
            // outlast the network coming back (`#406`).
            attempt_timeout: Duration::from_secs(5),
        }),
    );
    let op = engine.register_state("smb-outage", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    // Pauses the copy once `PAUSE_AFTER_FILES` have landed, once only: after
    // the resume, the copy has to run on into the outage.
    let pausing = Arc::clone(&state);
    let paused = std::sync::atomic::AtomicBool::new(false);
    let sink: Arc<dyn OperationEventSink> = Arc::new(ProgressTap {
        inner: Arc::clone(&events),
        tap: Box::new(move |event| {
            let pause = matches!(event.phase, WriteOperationPhase::Copying) && event.files_done >= PAUSE_AFTER_FILES;
            if pause && !paused.swap(true, Ordering::SeqCst) {
                pausing.pause_gate.pause();
            }
        }),
    });

    let copy_state = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let dest_path = PathBuf::from(&base);
    let mut copy = tokio::spawn(async move {
        copy_volumes_with_progress(
            sink,
            &op_id,
            &copy_state,
            source,
            &[PathBuf::from("batch")],
            through_proxy,
            &dest_path,
            &VolumeCopyConfig {
                conflict_resolution,
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await
        .map_err(|failure| format!("{:?}", failure.error))
    });

    // Mid-copy: the network goes away while the copy is paused, then the copy
    // carries on into it.
    wait_until_async(WAIT, "the copy to pause part-way", || state.pause_gate.is_paused()).await;
    proxy.cut();
    state.pause_gate.resume();
    wait_until_async(WAIT, "the copy to wait for the share", || {
        state.outage.waiting_for().is_some()
    })
    .await;

    // While the network is gone, the copy keeps waiting rather than failing.
    // allowed-test-sleep: the subject IS that time passing doesn't end the wait
    // while the network is down; there's no event for "still waiting".
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!copy.is_finished(), "a copy whose share is gone waits; it doesn't fail");
    assert!(state.outage.waiting_for().is_some(), "and says what it's waiting for");

    proxy.restore();
    let finished = tokio::time::timeout(WAIT, &mut copy).await;
    let result = match finished {
        Ok(joined) => joined.expect("the copy task itself must not panic"),
        Err(_) => {
            copy.abort();
            panic!("the copy didn't finish within {WAIT:?} of the network coming back");
        }
    };
    result.expect("a share that comes back within the budget must not fail the copy");

    for index in 0..FILE_COUNT {
        let path = PathBuf::from(&base).join("batch").join(format!("f-{index:02}.bin"));
        assert_eq!(
            hash_volume_file(&*direct, &path).await,
            hash_bytes(&content(index)),
            "{} must land byte-exact",
            path.display()
        );
    }
    let leftovers: Vec<String> = direct
        .list_directory(&PathBuf::from(&base).join("batch"), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.contains(".cmdr-tmp-") || name.contains("(recovered)"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staged partial survives the outage, and nothing is parked aside: {leftovers:?}"
    );

    ensure_clean(&direct, &base).await;
}

/// The big file's size: far past any single-shot write, so it always goes
/// through the staged writer and a temp of ours sits on the share when the
/// network goes.
const BIG_FILE_BYTES: usize = 64 * 1024 * 1024;

/// The copy loses the network once this much of the big file is through.
const CUT_AFTER_BYTES: u64 = 2 * 1024 * 1024;

/// How long the background cleanup waits for a share that's out. The cancel
/// must be reported well inside it, which is what tells "reported at once" from
/// "reported after the cleanup gave up".
const CLEANUP_WAIT: Duration = Duration::from_secs(20);

/// The most the cancel may take to be reported while the share is out.
const PROMPTLY: Duration = Duration::from_secs(5);

/// A Cancel while the share is out is reported at once, with the temp it left
/// named as "goes once the share is back", and the temp IS gone once the
/// network returns, without a launch or another transfer into that folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_cancel_while_the_share_is_out_reports_at_once_and_removes_the_temp_once_it_is_back() {
    let direct = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");

    let proxy = Arc::new(OutageProxy::start(guest_port()).await);
    let through_proxy: Arc<dyn Volume> =
        Arc::new(make_docker_volume_through(proxy.port(), crate::volume_host::host()).await);

    let local = tempfile::TempDir::new().expect("a local source dir");
    let big = content(0).into_iter().cycle().take(BIG_FILE_BYTES).collect::<Vec<u8>>();
    std::fs::write(local.path().join("big.bin"), &big).expect("the source file");
    let source: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    let engine = TestOperations::new();
    let state = Arc::new(
        new_operation_state(&engine, Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(60),
                cadence: CADENCE,
                poll_every: Duration::from_millis(20),
                attempt_timeout: Duration::from_secs(5),
            })
            .with_cleanup_policy(CleanupPolicy {
                wait: CLEANUP_WAIT,
                ..CleanupPolicy::default()
            }),
    );
    let op = engine.register_state("smb-outage-cancel", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    // Cuts the network part-way through the big file, from the copy's own
    // emit, so the temp is mid-write when it goes.
    let cutting = Arc::clone(&proxy);
    let cut = std::sync::atomic::AtomicBool::new(false);
    let sink: Arc<dyn OperationEventSink> = Arc::new(ProgressTap {
        inner: Arc::clone(&events),
        tap: Box::new(move |event| {
            let due = matches!(event.phase, WriteOperationPhase::Copying) && event.bytes_done >= CUT_AFTER_BYTES;
            if due && !cut.swap(true, Ordering::SeqCst) {
                cutting.cut();
            }
        }),
    });

    let copy_state = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let dest = Arc::clone(&through_proxy);
    let dest_path = PathBuf::from(&base);
    let mut copy = tokio::spawn(async move {
        copy_volumes_with_progress(
            sink,
            &op_id,
            &copy_state,
            source,
            &[PathBuf::from("big.bin")],
            dest,
            &dest_path,
            &VolumeCopyConfig {
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await
        .map_err(|failure| failure.error)
    });

    wait_until_async(WAIT, "the copy to wait for the share", || {
        state.outage.waiting_for().is_some()
    })
    .await;
    // The drop really happened: the share's own session reads it.
    assert_eq!(
        through_proxy.connection_state(),
        Some(ConnectionState::Disconnected),
        "the cut reached the share's session"
    );
    assert!(
        temps_in(&direct, &base).await.len() == 1,
        "the staged temp sits on the share while the network is out"
    );

    let cancelled_at = Instant::now();
    engine.cancel_write_operation(op.id(), false);
    let result = match tokio::time::timeout(PROMPTLY, &mut copy).await {
        Ok(joined) => joined.expect("the copy task itself must not panic"),
        Err(_) => {
            copy.abort();
            panic!("a Cancel while the share is out wasn't reported within {PROMPTLY:?}");
        }
    };
    let reported_after = cancelled_at.elapsed();
    assert!(
        matches!(result, Err(cmdr_operations::WriteOperationError::Cancelled { .. })),
        "a cancelled copy reports the cancel: {result:?}"
    );
    assert!(
        reported_after < CLEANUP_WAIT / 2,
        "the cancel doesn't wait for the cleanup ({reported_after:?})"
    );
    let cancelled = events.cancelled.lock_ignore_poison().clone();
    let leftovers = &cancelled
        .first()
        .expect("one write-cancelled")
        .rollback
        .staged_leftovers;
    // Read off the wire: the fate is what the frontend words the line by.
    let fates: Vec<serde_json::Value> = leftovers
        .iter()
        .map(|group| serde_json::to_value(group.fate).expect("a fate serializes"))
        .collect();
    assert_eq!(
        fates,
        vec![serde_json::json!("removedWhenBack")],
        "the temp goes once the share is back: {leftovers:?}"
    );
    assert_eq!(
        temps_in(&direct, &base).await.len(),
        1,
        "still there while the share is out"
    );

    // The network returns, and the share's session comes back the way a pane's
    // backoff brings it back: someone else's reconnect, which the background
    // pass is waiting for.
    proxy.restore();
    through_proxy
        .attempt_reconnect()
        .await
        .expect("the share reconnects once the network is back");
    tokio::time::timeout(WAIT, engine.leftover_cleanups_settled())
        .await
        .expect("the background cleanup settles once the share is back");

    let temps = temps_in(&direct, &base).await;
    assert!(temps.is_empty(), "no temp is left once the share is back: {temps:?}");

    ensure_clean(&direct, &base).await;
}

/// The `.cmdr-tmp-*` names in `dir` on the share, read through `volume`.
async fn temps_in(volume: &SmbVolume, dir: &str) -> Vec<String> {
    volume
        .list_directory(Path::new(dir), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect()
}
