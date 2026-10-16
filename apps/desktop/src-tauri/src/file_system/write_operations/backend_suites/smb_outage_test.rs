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
//! `#[ignore]`d like every `smb_integration_` test; the
//! `desktop-rust-integration-tests` check boots the Docker Samba stack and runs
//! it. Locally: `./apps/desktop/test/smb-servers/start.sh`, then
//! `cargo nextest run smb_integration_a_copy --run-ignored all`.

use super::outage_proxy_test_support::OutageProxy;
use super::smb_test_support::*;

use crate::file_system::write_operations::outage_slot::OutagePolicy;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::{
    CollectorEventSink, ConflictInfo, DryRunResult, OperationEventSink, ScanProgressEvent, VolumeCopyConfig,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent,
    WriteOperationPhase, WriteOperationState, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
    copy_volumes_with_progress,
};
use crate::test_support::wait_until_async;

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

/// Forwards everything to a collector, and pauses the operation once
/// [`PAUSE_AFTER_FILES`] have landed: the copy's own emit decides the moment, so
/// it can't finish before the test gets to cut the network.
struct PauseAfterFiles {
    inner: Arc<CollectorEventSink>,
    state: Arc<WriteOperationState>,
    /// Once only: after the resume, the copy has to run on into the outage.
    paused: std::sync::atomic::AtomicBool,
}

impl OperationEventSink for PauseAfterFiles {
    fn emit_progress(&self, event: WriteProgressEvent) {
        let pause = matches!(event.phase, WriteOperationPhase::Copying) && event.files_done >= PAUSE_AFTER_FILES;
        self.inner.emit_progress(event);
        if pause && !self.paused.swap(true, Ordering::SeqCst) {
            self.state.pause_gate.pause();
        }
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
    fn emit_conflict(&self, event: WriteConflictEvent) {
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
    // A direct connection seeds and verifies; the copy runs through the proxy.
    let direct = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");

    let proxy = OutageProxy::start(guest_port()).await;
    let through_proxy: Arc<dyn Volume> =
        Arc::new(make_docker_volume_through(proxy.port(), crate::volume_host::host()).await);

    let local = tempfile::TempDir::new().expect("a local source dir");
    let folder = local.path().join("batch");
    std::fs::create_dir(&folder).expect("the source folder");
    for index in 0..FILE_COUNT {
        std::fs::write(folder.join(format!("f-{index:02}.bin")), content(index)).expect("a source file");
    }
    let source: Arc<dyn Volume> = Arc::new(crate::file_system::volume::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(50)).with_outage_policy(OutagePolicy {
            budget: Duration::from_secs(60),
            cadence: CADENCE,
            poll_every: Duration::from_millis(20),
            // Well inside `WAIT`, so one handshake that never returns can't
            // outlast the network coming back (`#406`).
            attempt_timeout: Duration::from_secs(5),
        }),
    );
    let op = TestOperationGuard::register_state("smb-outage", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    let sink: Arc<dyn OperationEventSink> = Arc::new(PauseAfterFiles {
        inner: Arc::clone(&events),
        state: Arc::clone(&state),
        paused: std::sync::atomic::AtomicBool::new(false),
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
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staged partial survives the outage: {leftovers:?}"
    );

    ensure_clean(&direct, &base).await;
}
