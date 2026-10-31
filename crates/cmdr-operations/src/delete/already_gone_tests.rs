//! Deleting or trashing an item that's already gone counts as done.
//!
//! Regression anchor (ERR-X84HN, 2026-10-28): another app moved a file out of a
//! folder a pane was showing, the pane's watcher missed it, and the person's
//! permanent delete of that row stopped with `SourceNotFound` and auto-sent an
//! error report. The file was gone, which is what the person wanted.

use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::OperationEventSink;
use super::super::event_sinks::CollectorEventSink;
use super::super::state::{CachedScanResult, FileInfo};
use super::super::testing::TestOperations;
use super::super::types::{
    AlreadyGone, ScanPreviewPurpose, SourceItemOutcome, WriteCompleteEvent, WriteOperationConfig,
};
use super::walker::delete_files_with_progress_inner;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{CopyScanResult, EntryKind, InMemoryVolume, Volume, VolumeError};
use cmdr_operation_log::types::Initiator;

const WAIT: Duration = Duration::from_secs(5);

async fn settled_completion(events: &CollectorEventSink) -> WriteCompleteEvent {
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "an item that's already gone is not a failure: {:?}",
        events.errors.lock_ignore_poison()
    );
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(complete.len(), 1, "exactly one completion");
    complete[0].clone()
}

fn skipped_gone(events: &CollectorEventSink) -> Vec<(String, SourceItemOutcome, bool)> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|event| event.outcome == SourceItemOutcome::Skipped)
        .map(|event| (event.source_path.clone(), event.outcome, event.source_removed))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_delete_of_a_file_that_is_already_gone_completes() {
    let root = TestDir::new("already-gone-local-one");
    let gone = root.join("song.m4a");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone.clone()],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 1,
            name: Some("song.m4a".to_string()),
        })
    );
    assert_eq!(complete.files_processed, 0);
    assert_eq!(
        skipped_gone(&events),
        vec![(gone.display().to_string(), SourceItemOutcome::Skipped, true)],
        "the gone source is reported removed, so search snapshots drop it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_delete_deletes_what_is_there_and_reports_what_was_already_gone() {
    let root = TestDir::new("already-gone-local-mixed");
    let present = root.join("present.txt");
    fs::write(&present, b"here").expect("present file");
    let gone_a = root.join("gone-a.txt");
    let gone_b = root.join("gone-b.txt");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone_a, present.clone(), gone_b],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert!(!present.exists(), "the item that was there is deleted");
    assert_eq!(complete.files_processed, 1);
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 2,
            total_count: 3,
            name: None,
        }),
        "a name only rides along for a single gone item"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_with_nothing_gone_carries_no_already_gone_note() {
    let root = TestDir::new("already-gone-none");
    let present = root.join("present.txt");
    fs::write(&present, b"here").expect("present file");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![present],
            WriteOperationConfig::default(),
            None,
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(complete.already_gone, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_of_an_item_that_is_already_gone_completes() {
    let volume = Arc::new(InMemoryVolume::new("naspi"));
    volume.create_file(Path::new("/here.txt"), b"here").await.expect("file");
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-volume-{}", std::process::id());
    engine.volumes().register(&volume_id, volume.clone() as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![PathBuf::from("/here.txt"), PathBuf::from("/gone.txt")],
            WriteOperationConfig::default(),
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert!(
        !volume.exists(Path::new("/here.txt")).await,
        "the item that was there is deleted"
    );
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 2,
            name: Some("gone.txt".to_string()),
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_of_an_item_that_is_already_gone_completes_without_a_refusal() {
    let root = TestDir::new("already-gone-trash");
    let gone = root.join("gone.txt");
    let engine = TestOperations::new();
    let events = Arc::new(CollectorEventSink::new());

    engine
        .trash_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![gone],
            None,
            WriteOperationConfig::default(),
            Initiator::User,
            None,
        )
        .await
        .expect("start trash");

    let complete = settled_completion(&events).await;
    assert!(
        complete.refused.is_none(),
        "a gone item is not something the OS refused"
    );
    assert_eq!(
        complete.already_gone,
        Some(AlreadyGone {
            item_count: 1,
            total_count: 1,
            name: Some("gone.txt".to_string()),
        })
    );
}

/// The race inside a running delete: a file the scan listed vanishes before its
/// unlink. Its absence is what the delete was for, so the walk carries on.
#[test]
fn a_local_delete_carries_on_past_a_file_that_vanished_after_the_scan() {
    let root = TestDir::new("already-gone-race");
    let folder = root.join("folder");
    fs::create_dir(&folder).expect("folder");
    let stays = folder.join("a.txt");
    let vanishes = folder.join("b.txt");
    fs::write(&stays, b"a").expect("a");
    fs::write(&vanishes, b"b").expect("b");

    let engine = TestOperations::new();
    let mut files = Vec::new();
    for file in [&stays, &vanishes] {
        let metadata = fs::symlink_metadata(file).expect("scanned file exists");
        files.push(FileInfo::new(file.clone(), root.to_path_buf(), &metadata));
    }
    let preview = CachedScanResult::from_local_walk(
        ScanPreviewPurpose::Delete,
        vec![folder.clone()],
        files,
        vec![folder.clone()],
        2,
        2,
        vec![(
            folder.clone(),
            CopyScanResult {
                file_count: 2,
                dir_count: 0,
                total_bytes: 2,
                dedup_bytes: 2,
                top_level_is_directory: true,
                top_level_modified_at: None,
                top_level_created_at: None,
            },
        )],
        None,
    );
    let preview_id = "already-gone-race-preview".to_string();
    engine
        .stores()
        .previews()
        .insert_scan_result(preview_id.clone(), preview);
    fs::remove_file(&vanishes).expect("another app takes b.txt");

    let op_id = "already-gone-race-op";
    let op = engine.register_state(op_id, engine.new_state());
    let sink = CollectorEventSink::new();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_files_with_progress_inner(&sink, op_id, op.state(), std::slice::from_ref(&folder), &config);

    assert!(result.is_ok(), "{result:?}");
    assert!(
        !folder.exists(),
        "the rest of the folder is deleted, the folder with it"
    );
    assert_eq!(sink.complete.lock_ignore_poison().len(), 1);
}

/// Counts the top-level probes (`get_metadata` and `entry_kind`) per path, and
/// forwards everything a volume delete asks to an `InMemoryVolume`.
struct ProbeCountingVolume {
    inner: InMemoryVolume,
    probes: std::sync::Mutex<std::collections::HashMap<PathBuf, usize>>,
}

impl ProbeCountingVolume {
    fn count(&self, path: &Path) {
        *self.probes.lock_ignore_poison().entry(path.to_path_buf()).or_default() += 1;
    }
    fn probes_of(&self, path: &str) -> usize {
        self.probes
            .lock_ignore_poison()
            .get(Path::new(path))
            .copied()
            .unwrap_or(0)
    }
}

impl Volume for ProbeCountingVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.is_directory(path)
    }
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        self.count(path);
        self.inner.entry_kind(path)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
}

/// Regression anchor: the already-gone probe came on top of the walk's own
/// top-level probe, so an S3 delete sent a second HEAD (and a folder a second
/// listing) per selected item, more than its cost estimate counts
/// (`s3_integration_the_engine_sends_what_the_estimate_counts_*`). The probe's
/// answer now feeds the walk: one probe per top-level item, and what's there
/// still goes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_probes_each_selected_item_once() {
    let volume = Arc::new(ProbeCountingVolume {
        inner: InMemoryVolume::new("bucket"),
        probes: std::sync::Mutex::new(std::collections::HashMap::new()),
    });
    volume
        .inner
        .create_file(Path::new("/file.txt"), b"x")
        .await
        .expect("file");
    volume
        .inner
        .create_directory(Path::new("/folder"))
        .await
        .expect("folder");
    volume
        .inner
        .create_file(Path::new("/folder/child.txt"), b"y")
        .await
        .expect("child");
    let engine = TestOperations::new();
    let volume_id = format!("already-gone-probe-once-{}", std::process::id());
    engine.volumes().register(&volume_id, volume.clone() as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());

    engine
        .delete_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![PathBuf::from("/file.txt"), PathBuf::from("/folder")],
            WriteOperationConfig::default(),
            Some(volume_id),
            Initiator::User,
            None,
        )
        .await
        .expect("start delete");

    let complete = settled_completion(&events).await;
    assert_eq!(complete.already_gone, None);
    assert!(
        !volume.inner.exists(Path::new("/file.txt")).await,
        "the file is deleted"
    );
    assert!(
        !volume.inner.exists(Path::new("/folder")).await,
        "the folder is deleted"
    );
    assert_eq!(
        volume.probes_of("/file.txt"),
        1,
        "{:?}",
        volume.probes.lock_ignore_poison()
    );
    assert_eq!(
        volume.probes_of("/folder"),
        1,
        "{:?}",
        volume.probes.lock_ignore_poison()
    );
}
