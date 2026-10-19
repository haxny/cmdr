//! Every engine walk that deletes a folder it walked says so, through
//! `Volume::delete_directory`, ❌ never a plain `delete`.
//!
//! On an object store one name can hold a file and a markerless folder
//! (`notes` beside `notes/…`), and the folder is gone the moment its last
//! child is, so a plain `delete` of the folder's path finds only the file and
//! deletes that: a file nobody selected (S3, issue #510). The backend can
//! refuse that only when the caller tells it a folder is meant.
//!
//! A spy over `InMemoryVolume` records every plain `delete` that lands on a
//! folder, and every `delete_directory`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use tokio_util::sync::CancellationToken;

use super::cleanup::{TreeRemoval, remove_tree};
use super::{CarriedSource, stamp_source, sweep_carried_source};
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::WriteOperationConfig;

struct FolderDeleteSpy {
    inner: InMemoryVolume,
    /// Folders a plain `delete` reached.
    blind: Mutex<Vec<PathBuf>>,
    /// What `delete_directory` was asked for.
    as_folders: Mutex<Vec<PathBuf>>,
}

impl FolderDeleteSpy {
    /// `/d/a.txt` and `/d/sub/b.txt`.
    async fn seeded() -> Arc<Self> {
        let inner = InMemoryVolume::new("spy");
        inner.create_directory(Path::new("/d")).await.unwrap();
        inner.create_directory(Path::new("/d/sub")).await.unwrap();
        inner.create_file(Path::new("/d/a.txt"), b"a").await.unwrap();
        inner.create_file(Path::new("/d/sub/b.txt"), b"bb").await.unwrap();
        Arc::new(Self {
            inner,
            blind: Mutex::new(Vec::new()),
            as_folders: Mutex::new(Vec::new()),
        })
    }

    fn blind(&self) -> Vec<PathBuf> {
        self.blind.lock().unwrap().clone()
    }

    fn as_folders(&self) -> Vec<PathBuf> {
        let mut folders = self.as_folders.lock().unwrap().clone();
        folders.sort();
        folders
    }

    /// Both folders went, each as a folder, and nothing deleted one blind.
    async fn assert_folders_deleted_as_folders(&self) {
        assert_eq!(
            self.blind(),
            Vec::<PathBuf>::new(),
            "a folder deleted by a plain `delete`"
        );
        assert_eq!(self.as_folders(), [PathBuf::from("/d"), PathBuf::from("/d/sub")]);
        assert!(!self.inner.exists(Path::new("/d")).await, "the folder went");
    }
}

impl Volume for FolderDeleteSpy {
    crate::forward_volume_methods!(
        inner => name,
        root,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        scan_for_copy,
        scan_for_copy_batch,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.inner.is_directory(path).await.unwrap_or(false) {
                self.blind.lock().unwrap().push(path.to_path_buf());
            }
            self.inner.delete(path).await
        })
    }

    fn delete_directory<'a>(
        &'a self,
        path: &'a Path,
        _cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.as_folders.lock().unwrap().push(path.to_path_buf());
        self.inner.delete(path)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_delete_walk_deletes_its_folders_as_folders() {
    let spy = FolderDeleteSpy::seeded().await;
    let volume: Arc<dyn Volume> = spy.clone();
    let engine = TestOperations::new();
    engine.volumes().register("spy", volume.clone());
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));

    crate::delete::delete_volume_files_with_progress_inner(
        volume,
        "spy",
        &CollectorEventSink::new(),
        "folder-kind-delete",
        &state,
        &[PathBuf::from("/d")],
        &WriteOperationConfig::default(),
    )
    .await
    .expect("the delete runs");

    spy.assert_folders_deleted_as_folders().await;
}

#[tokio::test]
async fn a_moves_source_sweep_deletes_its_folders_as_folders() {
    let spy = FolderDeleteSpy::seeded().await;
    let volume: Arc<dyn Volume> = spy.clone();
    let carried = stamp_source(&volume, Path::new("/d")).await.expect("the walk stamps");
    assert!(matches!(carried, CarriedSource::Folder(_)));

    sweep_carried_source(&volume, Path::new("/d"), &carried, &Default::default())
        .await
        .expect("the sweep runs");

    spy.assert_folders_deleted_as_folders().await;
}

#[tokio::test]
async fn a_tree_removal_deletes_its_folders_as_folders() {
    let spy = FolderDeleteSpy::seeded().await;
    let volume: Arc<dyn Volume> = spy.clone();

    remove_tree(&volume, Path::new("/d"), TreeRemoval::UserChoseOverwriteAcrossTypes)
        .await
        .expect("the tree goes");

    spy.assert_folders_deleted_as_folders().await;
}
