//! What a safe-replace's landing does when it can't simply swap the new bytes
//! over the original (`WriteStaging::StageOverOriginal`, delete-first).
//!
//! Once the original's delete has gone through, the temp is the ONLY complete
//! copy in existence. It must leave `.cmdr-tmp-*` space when the rename after
//! the delete fails, because `cleanup.rs::reap_stale_transfer_temps` matches on
//! that marker plus an age and runs at the start of every transfer into the
//! directory. And a delete the destination refused must never cost the user
//! their original.

use super::cleanup::reap_stale_transfer_temps;
use super::faulty_volume::forward_volume_methods;
use super::strategy::WriteStaging;
use crate::event_sinks::CollectorEventSink;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::recovered_name::{FinalizeFailure, LandingAftermath};
use crate::transfer::staged_write::StagedWrite;
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

/// A destination whose `rename` refuses to land anything at `fails_onto`, and
/// forwards everything else. Models the disconnect at the exact instant the
/// swap renames the completed temp onto the original's name: `delete(orig)`
/// goes through, the rename that follows doesn't.
struct RenameRefusesOne {
    inner: Arc<InMemoryVolume>,
    fails_onto: PathBuf,
}

impl Volume for RenameRefusesOne {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory, delete, get_space_info, supports_streaming, open_read_stream,
        supports_unknown_length_writes, write_from_stream,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if to != self.fails_onto {
            return self.inner.rename(from, to, force);
        }
        Box::pin(async {
            Err(VolumeError::IoError {
                message: "simulated disconnect during the swap's rename".to_string(),
                raw_os_error: None,
            })
        })
    }
}

/// `/notes.txt` holds the user's old file, as every cell starts.
async fn dest_holding_the_original(inner: InMemoryVolume) -> Arc<InMemoryVolume> {
    let inner = Arc::new(inner.with_space_info(10_000_000, 10_000_000));
    inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    inner
}

/// Stages a write over `/notes.txt`, puts the complete new bytes on its temp
/// exactly as the streaming write leaves them, and lands it. Answers the temp
/// and the landing's outcome.
async fn swap_over_the_original(
    engine: &TestOperations,
    inner: &InMemoryVolume,
    dest: &Arc<dyn Volume>,
) -> (PathBuf, Result<(), FinalizeFailure>) {
    let state = Arc::new(unwatched_state(engine));
    let original = state.scope().host().announce_on(&**dest, Path::new("/notes.txt"));
    let staged = StagedWrite::begin(&state, &**dest, original, WriteStaging::StageOverOriginal);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();
    (temp, staged.commit_without_a_wait(dest, 3).await)
}

/// What `path` holds right now, or `None` if it isn't there.
async fn contents(volume: &InMemoryVolume, path: &Path) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(path).await.ok()?;
    let mut buf = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk);
    }
    Some(buf)
}

async fn names_in_root(volume: &InMemoryVolume) -> Vec<String> {
    volume
        .list_directory(Path::new("/"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// A destination whose rename onto `/notes.txt` fails after the delete went
/// through.
async fn dest_refusing_the_rename() -> (Arc<InMemoryVolume>, Arc<dyn Volume>) {
    let inner = dest_holding_the_original(InMemoryVolume::new("Dest")).await;
    let dest: Arc<dyn Volume> = Arc::new(RenameRefusesOne {
        inner: Arc::clone(&inner),
        fails_onto: PathBuf::from("/notes.txt"),
    });
    (inner, dest)
}

/// The rescue: the complete new bytes take a real filename next to where they
/// were meant to land, and nothing is left wearing a temp name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_whose_rename_fails_moves_the_new_data_out_of_temp_space() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_rename().await;

    let (_, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("the swap's rename is rigged to fail");

    assert_eq!(
        failure.aftermath,
        Some(LandingAftermath::NewDataKeptAt(PathBuf::from("/notes (recovered).txt"))),
        "the failure has to name where the user's new file actually is"
    );
    assert_eq!(
        contents(&inner, Path::new("/notes (recovered).txt")).await.as_deref(),
        Some(&b"NEW"[..]),
        "and that path has to hold the complete new bytes"
    );
    let names = names_in_root(&inner).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "nothing may still wear a temp name; found {names:?}"
    );
}

/// The point of the rescue: the hourly stale-temp reap can't touch the rescued
/// file, however long it sits there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_stale_temp_reap_cannot_touch_rescued_data() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_rename().await;
    let (_, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    landed.expect_err("the swap's rename is rigged to fail");

    // A day later, the next transfer into the same folder runs the reap.
    for name in names_in_root(&inner).await {
        inner.set_modified_at(&PathBuf::from("/").join(&name), Some(1));
    }
    reap_stale_transfer_temps(&unwatched_state(&engine), &dest, Path::new("/")).await;

    assert_eq!(
        contents(&inner, Path::new("/notes (recovered).txt")).await.as_deref(),
        Some(&b"NEW"[..]),
        "the only copy of the user's new file must survive the reap"
    );
}

/// A ` (recovered)` name already in use continues the house ` (N)` series rather
/// than replacing whatever is there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_taken_recovered_name_continues_the_house_series() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_rename().await;
    inner
        .create_file(Path::new("/notes (recovered).txt"), b"AN EARLIER RESCUE")
        .await
        .unwrap();

    let (_, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("the swap's rename is rigged to fail");

    assert_eq!(
        failure.aftermath,
        Some(LandingAftermath::NewDataKeptAt(PathBuf::from(
            "/notes (recovered) (1).txt"
        )))
    );
    assert_eq!(
        contents(&inner, Path::new("/notes (recovered).txt")).await.as_deref(),
        Some(&b"AN EARLIER RESCUE"[..]),
        "the earlier rescue is somebody's file too"
    );
}

/// When the destination refuses every rename — the same dead link that failed
/// the swap — the bytes stay under the temp name and the failure says so.
/// Reporting the recovered path we never created would send the user to a file
/// that isn't there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rescue_that_cannot_rename_reports_the_temp_it_left() {
    let engine = TestOperations::new();
    let inner = dest_holding_the_original(
        InMemoryVolume::new("Dest").with_rename_failing(VolumeError::DeviceDisconnected("gone".to_string())),
    )
    .await;
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let (temp, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("every rename fails here");

    assert_eq!(failure.aftermath, Some(LandingAftermath::NewDataKeptAt(temp.clone())));
    assert_eq!(contents(&inner, &temp).await.as_deref(), Some(&b"NEW"[..]));
}

/// A destination whose `delete` of `refuses` answers `PermissionDenied`, and
/// forwards everything else. With `goes_through`, the delete HAPPENS and the
/// answer still says it didn't: a transport that dropped the response after
/// the server acted.
struct DeleteRefusesOne {
    inner: Arc<InMemoryVolume>,
    refuses: PathBuf,
    goes_through: bool,
}

impl Volume for DeleteRefusesOne {
    forward_volume_methods!(inner =>
        name, root, lane_key, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory, create_directory_all, rename, get_space_info, local_path, supports_streaming,
        supports_export, supports_local_fs_access, operations_are_local, max_concurrent_ops,
        create_directory_errors_on_existing_dir, scan_for_copy, scan_for_copy_batch, scan_for_conflicts,
        open_read_stream, supports_unknown_length_writes, write_from_stream, write_is_single_shot,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path != self.refuses {
                return self.inner.delete(path).await;
            }
            if self.goes_through {
                self.inner.delete(path).await?;
            }
            Err(VolumeError::PermissionDenied {
                path: path.display().to_string(),
                raw_os_error: None,
            })
        })
    }
}

/// The destination refuses to delete `/notes.txt`.
async fn dest_refusing_the_delete(goes_through: bool) -> (Arc<InMemoryVolume>, Arc<dyn Volume>) {
    let inner = dest_holding_the_original(InMemoryVolume::new("Dest")).await;
    let dest: Arc<dyn Volume> = Arc::new(DeleteRefusesOne {
        inner: Arc::clone(&inner),
        refuses: PathBuf::from("/notes.txt"),
        goes_through,
    });
    (inner, dest)
}

/// The server wouldn't let the original go, so the swap can't happen: the
/// original stays exactly as it was, the refusal reaches the caller typed, and
/// the complete-but-unplaced temp goes at once rather than sitting in the
/// user's folder until an hourly reap. The source still holds those bytes, so
/// removing them loses nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_delete_keeps_the_original_and_takes_its_own_temp_away() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_delete(false).await;

    let (_, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("the delete is rigged to be refused");

    assert!(
        matches!(failure.error, VolumeError::PermissionDenied { .. }),
        "the refusal must arrive typed, got {:?}",
        failure.error
    );
    assert!(
        failure.aftermath.is_none(),
        "nothing was cleared, so nothing was rescued"
    );
    assert_eq!(
        contents(&inner, Path::new("/notes.txt")).await.as_deref(),
        Some(&b"OLD"[..]),
        "❗ the user's file must be exactly as it was"
    );
    assert_eq!(
        names_in_root(&inner).await,
        vec!["notes.txt".to_string()],
        "no `.cmdr-tmp-*` may be left beside it"
    );
}

/// A delete that ANSWERED with a refusal but went through anyway (the response
/// lost on the way back) leaves the name empty. Discarding the temp then would
/// leave neither file at the destination; landing it is what the user asked for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_that_went_through_despite_its_answer_still_lands_the_new_bytes() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_delete(true).await;

    let (_, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    landed.expect("the original is gone, so the swap completes");

    assert_eq!(
        contents(&inner, Path::new("/notes.txt")).await.as_deref(),
        Some(&b"NEW"[..])
    );
    assert_eq!(names_in_root(&inner).await, vec!["notes.txt".to_string()]);
}

/// When the destination can't even say whether the original survived, the temp
/// stays: if the original IS gone, the temp is the only copy at the destination,
/// and this is not the moment to guess.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_delete_nobody_can_confirm_keeps_the_temp() {
    let engine = TestOperations::new();
    let (inner, dest) = dest_refusing_the_delete(false).await;
    inner.set_stat_failing(Path::new("/notes.txt"));

    let (temp, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("the delete is rigged to be refused");

    assert!(matches!(failure.error, VolumeError::PermissionDenied { .. }));
    assert_eq!(contents(&inner, &temp).await.as_deref(), Some(&b"NEW"[..]));
}

/// A refused replace through the whole copy pipeline, on both drivers: the
/// user's file stays, the copy fails typed, and no staging is left behind.
async fn copy_over_a_file_the_server_wont_let_go(engine: &TestOperations, names: &[&str]) {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for name in names {
        source
            .create_file(&Path::new("/").join(name), format!("SRC-{name}").as_bytes())
            .await
            .unwrap();
    }
    let inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/dest")).await.unwrap();
    inner
        .create_file(Path::new("/dest/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    let dest: Arc<dyn Volume> = Arc::new(DeleteRefusesOne {
        inner: Arc::clone(&inner),
        refuses: PathBuf::from("/dest/notes.txt"),
        goes_through: false,
    });

    let sources: Vec<PathBuf> = names.iter().map(|n| Path::new("/").join(n)).collect();
    let result = super::copy::copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-refused-replace",
        &Arc::new(engine.new_operation_state(std::time::Duration::from_millis(50))),
        source as Arc<dyn Volume>,
        &sources,
        dest,
        Path::new("/dest"),
        &VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..VolumeCopyConfig::default()
        },
    )
    .await;
    let Err(failure) = result else {
        panic!("a replace the destination refused must not report success");
    };
    assert!(
        matches!(failure.error, WriteOperationError::PermissionDenied { .. }),
        "got {:?}",
        failure.error
    );
    assert_eq!(
        contents(&inner, Path::new("/dest/notes.txt")).await.as_deref(),
        Some(&b"THE USER'S FILE"[..]),
        "❗ the user's file must be exactly as it was"
    );
    let leftovers: Vec<String> = inner
        .list_directory(Path::new("/dest"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.contains(".cmdr-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "no staging may be left behind: {leftovers:?}");
}

/// One source, so the SERIAL driver runs it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serial_a_refused_replace_leaves_no_staging() {
    let engine = TestOperations::new();
    copy_over_a_file_the_server_wont_let_go(&engine, &["notes.txt"]).await;
}

/// Three sources, so the CONCURRENT driver runs them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_a_refused_replace_leaves_no_staging() {
    let engine = TestOperations::new();
    copy_over_a_file_the_server_wont_let_go(&engine, &["a.txt", "notes.txt", "z.txt"]).await;
}

/// A swap that fails on the DELETE rescued nothing and lost nothing: the
/// destination still holds the user's file. Here the destination refuses EVERY
/// delete, so the temp can't be taken away either; it stays, recorded as a temp
/// again, for the sweeps, which is safe because the source still holds its
/// bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_that_never_deleted_the_original_reports_no_rescue() {
    let engine = TestOperations::new();
    let inner = dest_holding_the_original(InMemoryVolume::new("Dest").with_delete_failing()).await;
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let (temp, landed) = swap_over_the_original(&engine, &inner, &dest).await;
    let failure = landed.expect_err("the delete is rigged to fail");

    assert!(
        failure.aftermath.is_none(),
        "nothing was rescued, because nothing was lost"
    );
    assert_eq!(
        contents(&inner, Path::new("/notes.txt")).await.as_deref(),
        Some(&b"OLD"[..])
    );
    assert_eq!(
        contents(&inner, &temp).await.as_deref(),
        Some(&b"NEW"[..]),
        "a temp the destination won't delete stays, whole, for the sweeps"
    );
}

/// The unused import guard: `FileEntry` is what `list_directory` answers with,
/// and the forwarding macro names the type in its expansion.
#[allow(dead_code, reason = "the macro expansion names the type")]
type ListingRow = FileEntry;

/// A state no operation is watching: no journal, no persisted ledger, so the
/// landing is all that happens.
fn unwatched_state(engine: &TestOperations) -> WriteOperationState {
    engine.new_operation_state(std::time::Duration::ZERO)
}
