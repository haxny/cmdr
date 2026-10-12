//! A copy to a share that drops mid-transfer waits for it to come back and
//! finishes, through the real `copy_volumes_with_progress` pipeline (`#398`).
//!
//! The wait itself is pinned in `transfer/volume_outage_tests.rs`. These cells
//! pin where the pipeline asks for it: the byte path (both drivers), and the
//! start of a transfer whose destination is already gone (a Retry while the NAS
//! is still down, or a job admitted mid-outage).

use super::*;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::outage_slot::OutagePolicy;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationError};
use crate::test_support::wait_until_async;

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(budget: Duration) -> Arc<WriteOperationState> {
    Arc::new(
        WriteOperationState::new(Duration::from_millis(50)).with_outage_policy(OutagePolicy {
            budget,
            cadence: CADENCE,
            poll_every: Duration::from_millis(5),
        }),
    )
}

/// A source with `count` files, both as a folder (`/batch/…`) and at the top
/// level (`/f<n>.txt`), so a cell can pick the serial walk or the concurrent
/// window.
async fn source_with(count: usize) -> Arc<dyn Volume> {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/batch")).await.unwrap();
    for n in 0..count {
        let body = format!("file {n}");
        source
            .create_file(Path::new(&format!("/batch/f{n}.txt")), body.as_bytes())
            .await
            .unwrap();
        source
            .create_file(Path::new(&format!("/f{n}.txt")), body.as_bytes())
            .await
            .unwrap();
    }
    source
}

fn nas(lane: &str) -> (Arc<InMemoryVolume>, Arc<OutageVolume>, Arc<dyn Volume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    let dest: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    (inner, nas, dest)
}

fn config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

/// Brings the share back once the operation has started waiting for it.
fn come_back_when_waited_for(state: &Arc<WriteOperationState>, nas: &Arc<OutageVolume>) {
    let state = Arc::clone(state);
    let nas = Arc::clone(nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        nas.become_reachable();
    });
}

async fn landed(inner: &Arc<InMemoryVolume>, path: &str) -> bool {
    inner.exists(Path::new(path)).await
}

/// The commonest copy there is: one folder, so the SERIAL driver and the merge
/// walk. The share drops as the second file starts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_copy_waits_out_a_dropped_share_and_finishes() {
    let source = source_with(4).await;
    let (inner, nas, dest) = nas("outage-copy-folder");
    nas.drop_at_write(1);
    let state = outage_state(Duration::from_secs(30));
    let op = TestOperationGuard::register_state("outage-copy-folder", Arc::clone(&state));
    come_back_when_waited_for(&state, &nas);

    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back within the budget must not fail the copy");

    for n in 0..4 {
        assert!(landed(&inner, &format!("/batch/f{n}.txt")).await, "f{n} must arrive");
    }
    assert!(nas.reconnect_attempts() >= 1, "the copy reconnected the share itself");
}

/// Several top-level files: the CONCURRENT window, where every in-flight task
/// meets the outage at once and each waits on the same share.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_file_copy_waits_out_a_dropped_share_and_finishes() {
    let source = source_with(5).await;
    let (inner, nas, dest) = nas("outage-copy-window");
    nas.drop_at_write(2);
    let state = outage_state(Duration::from_secs(30));
    let op = TestOperationGuard::register_state("outage-copy-window", Arc::clone(&state));
    come_back_when_waited_for(&state, &nas);

    let sources: Vec<PathBuf> = (0..5).map(|n| PathBuf::from(format!("/f{n}.txt"))).collect();
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id(),
        &state,
        source,
        &sources,
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back within the budget must not fail the copy");

    for n in 0..5 {
        assert!(landed(&inner, &format!("/f{n}.txt")).await, "f{n} must arrive");
    }
}

/// The share never comes back: after the budget, the copy fails exactly as it
/// did before it knew how to wait, with what landed still there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_gives_up_like_before_when_the_share_stays_gone() {
    let source = source_with(4).await;
    let (inner, nas, dest) = nas("outage-copy-gone");
    nas.drop_at_write(1);
    let state = outage_state(Duration::from_millis(300));
    let op = TestOperationGuard::register_state("outage-copy-gone", Arc::clone(&state));

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect_err("a share that never comes back still fails the copy");

    assert!(
        matches!(failure.error, WriteOperationError::DeviceDisconnected { .. }),
        "the same failure as before the wait existed: {:?}",
        failure.error
    );
    assert!(nas.reconnect_attempts() > 1, "after really trying");
    // Nothing of the user's is touched by giving up: the destination folder
    // holds only whole files under real names, never a half-written one.
    nas.reconnected_elsewhere();
    let listing = inner
        .list_directory(Path::new("/batch"), None)
        .await
        .unwrap_or_default();
    assert!(
        listing
            .iter()
            .filter(|entry| !entry.name.contains(".cmdr-tmp-"))
            .all(|entry| entry.size == Some("file 0".len() as u64)),
        "every file under a real name is whole: {listing:?}"
    );
}

/// Cancel during the wait ends the copy at once, as a cancel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_wait_cancels_the_copy() {
    let source = source_with(4).await;
    let (_inner, nas, dest) = nas("outage-copy-cancel");
    nas.drop_at_write(1);
    let state = outage_state(Duration::from_secs(60));
    let op = TestOperationGuard::register_state("outage-copy-cancel", Arc::clone(&state));
    let watched = Arc::clone(&state);
    let op_id = op.id().to_owned();
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        crate::file_system::write_operations::state::cancel_write_operation(&op_id, false);
    });

    let started = Instant::now();
    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect_err("a cancelled copy doesn't finish");

    assert!(
        matches!(failure.error, WriteOperationError::Cancelled { .. }),
        "a cancel is reported as one, never as the outage: {:?}",
        failure.error
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "and doesn't wait out the budget"
    );
}

/// A copy that STARTS with its destination gone (a Retry while the NAS is still
/// down, a job admitted mid-outage) waits for it before its first step, rather
/// than failing on it at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_that_starts_while_the_share_is_gone_waits_for_it() {
    let source = source_with(2).await;
    let (inner, nas, dest) = nas("outage-copy-start");
    nas.go_away();
    let state = outage_state(Duration::from_secs(30));
    let op = TestOperationGuard::register_state("outage-copy-start", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    come_back_when_waited_for(&state, &nas);

    copy_volumes_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        op.id(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back must let the copy start");

    assert!(landed(&inner, "/batch/f1.txt").await);
    assert!(
        events.progress.lock_ignore_poison().iter().any(|e| e
            .activity
            .as_ref()
            .and_then(|a| a.reconnecting_volume.as_deref())
            == Some("Naspolya")),
        "the windows were told what the copy was waiting for"
    );
}

/// Copies `/batch` (four files) onto a share armed to drop around the landing
/// rename of the second file, and hands back the outcome. With `theirs`, someone
/// else saves that file's name with those bytes while the share is gone.
async fn copy_through_a_drop_at_landing(
    lane: &str,
    side: RenameDrop,
    theirs: Option<&'static [u8]>,
) -> (Arc<InMemoryVolume>, Result<(), WriteOperationError>) {
    let source = source_with(4).await;
    let (inner, nas, dest) = nas(lane);
    nas.drop_at_rename(1, side);
    let state = outage_state(Duration::from_secs(30));
    let op = TestOperationGuard::register_state(lane, Arc::clone(&state));
    let watched = Arc::clone(&state);
    let behind_the_share = Arc::clone(&inner);
    let share = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        if let Some(theirs) = theirs {
            // Ours landed before the drop; they save over it.
            behind_the_share
                .delete(Path::new("/batch/f1.txt"))
                .await
                .expect("ours is there to save over");
            behind_the_share
                .create_file(Path::new("/batch/f1.txt"), theirs)
                .await
                .expect("their save");
        }
        share.become_reachable();
    });

    let outcome = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (inner, outcome)
}

async fn names_in_batch(inner: &Arc<InMemoryVolume>) -> Vec<String> {
    let mut names: Vec<String> = inner
        .list_directory(Path::new("/batch"), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

async fn content_of(inner: &Arc<InMemoryVolume>, path: &str) -> Vec<u8> {
    let mut stream = inner.open_read_stream(Path::new(path)).await.expect("the file opens");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("a chunk"));
    }
    bytes
}

/// The server performed the landing rename and the drop swallowed its reply:
/// the file is whole at its name and the temp is gone. Once the share is back,
/// the copy recognizes the file as the one IT just landed and counts it done,
/// rather than re-copying it into its own file and failing with
/// `DestinationExists` (the live-SMB cell
/// `smb_outage_test::smb_integration_a_copy_waits_out_a_dropped_network_and_finishes`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_the_drop_swallowed_counts_as_landed() {
    let (inner, outcome) =
        copy_through_a_drop_at_landing("outage-landed-unanswered", RenameDrop::AfterTheServerPerformsIt, None).await;

    outcome.expect("a landing the server performed is the copy's own file, not a conflict");
    for n in 0..4 {
        assert_eq!(
            content_of(&inner, &format!("/batch/f{n}.txt")).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
    }
    assert_eq!(
        names_in_batch(&inner).await,
        ["f0.txt", "f1.txt", "f2.txt", "f3.txt"],
        "no staged partial survives"
    );
}

/// The landing rename never reached the server: the temp is still there, so the
/// file runs again and lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_the_drop_stopped_runs_the_file_again() {
    let (inner, outcome) =
        copy_through_a_drop_at_landing("outage-landing-unsent", RenameDrop::BeforeTheServerSeesIt, None).await;

    outcome.expect("a landing that never happened runs again once the share is back");
    assert_eq!(content_of(&inner, "/batch/f1.txt").await, b"file 1".to_vec());
    assert_eq!(
        names_in_batch(&inner).await,
        ["f0.txt", "f1.txt", "f2.txt", "f3.txt"],
        "no staged partial survives"
    );
}

/// The landing's reply was lost AND, while the share was gone, someone else
/// replaced the file at that name with theirs. The copy can't call that file its
/// own, so it takes the normal late-clash path: it fails naming the conflict and
/// leaves their file alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_someone_else_took_during_the_outage_stays_a_conflict() {
    const THEIRS: &[u8] = b"someone else's file, saved while the share was away";
    let (inner, outcome) = copy_through_a_drop_at_landing(
        "outage-landed-then-taken",
        RenameDrop::AfterTheServerPerformsIt,
        Some(THEIRS),
    )
    .await;

    let error = outcome.expect_err("a file the copy can't prove its own is never claimed");
    assert!(
        matches!(error, WriteOperationError::DestinationExists { .. }),
        "the clash is reported as one: {error:?}"
    );
    assert_eq!(
        content_of(&inner, "/batch/f1.txt").await,
        THEIRS.to_vec(),
        "theirs survives"
    );
}
