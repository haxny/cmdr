//! Headless tests for the generic archive-edit driver (`archive_edit_start`) and
//! the in-archive delete route: they run the mutator as a managed op and emit the
//! right terminal events, with no Tauri runtime (a `CollectorEventSink` captures
//! events).

use super::test_support::*;
use super::*;
use crate::file_system::write_operations::installed;
use cmdr_archive::mutator::{AddEntry, AddSource};

#[tokio::test]
async fn a_successful_edit_rewrites_the_archive_and_emits_complete_then_settled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_simple_zip(&path, "keep.txt", b"keep");

    let events = Arc::new(CollectorEventSink::new());
    // A unique parent keeps this op off any shared operation-manager lane (see
    // `test_support::unique_lane_id`). The root-parent → `None`-settle special
    // case is pinned by `move_out_tests`, which can pass a `"root"` settle id
    // WITHOUT reserving the `"root"` lane (its lanes come from the volume objects).
    let parent = unique_lane_id();
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: parent.clone(),
        changeset: Changeset {
            adds: vec![AddEntry {
                inner_path: "added.txt".to_string(),
                source: AddSource::Bytes(b"new bytes".to_vec()),
            }],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        skipped_count: 0,
        preview_id: None,
    };

    let start = installed()
        .archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");
    assert_eq!(start.operation_type, WriteOperationType::ArchiveEdit);

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;

    // The archive was actually rewritten.
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert_eq!(read_entry(&path, "added.txt").as_deref(), Some(b"new bytes".as_slice()));

    {
        let complete = events.complete.lock_ignore_poison();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].operation_type, WriteOperationType::ArchiveEdit);
    }
    // No error, and settle fired for the same op.
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "no write-error on success"
    );
    wait_until_async(Duration::from_secs(5), "the write-settled event", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    // A NON-root parent carries its volume id in the settle event (the FE clears
    // that drive's eject guard on it). The `root` → `None` case is pinned by
    // `move_out_tests`.
    assert_eq!(events.settled.lock_ignore_poison()[0].volume_id, Some(parent));
}

#[tokio::test]
async fn route_archive_delete_removes_entries_and_completes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        for name in ["keep.txt", "drop.txt"] {
            writer.start_file(name, SimpleFileOptions::default()).expect("start");
            writer.write_all(name.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
    }

    let events = Arc::new(CollectorEventSink::new());
    // The FE sends full paths inside the archive.
    let sources = vec![path.join("drop.txt")];
    let parent = unique_lane_id();
    installed()
        .route_archive_delete(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            &sources,
            &parent,
            0,
            None,
        )
        .await
        .expect("start delete");

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep.txt".as_slice()));
}

#[tokio::test]
async fn deleting_inside_a_remote_archive_reports_its_upload() {
    let archive_path = PathBuf::from("/device/bundle.zip");
    let (parent_id, parent) = register_remote_zip(&archive_path, &[("keep.txt", b"keep"), ("drop.txt", b"drop")]).await;

    let events = Arc::new(CollectorEventSink::new());
    installed()
        .route_archive_delete(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            &[archive_path.join("drop.txt")],
            &parent_id,
            0,
            None,
        )
        .await
        .expect("start delete");
    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;

    assert!(
        read_remote_entry(parent.as_ref(), &archive_path, "drop.txt")
            .await
            .is_none()
    );
    assert_upload_reported_honestly(&events, parent.as_ref(), &archive_path).await;
    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn route_archive_delete_reports_the_deleted_count_not_the_retained_count() {
    // Deleting ONE entry from a 3-entry zip must report `files_processed == 1`
    // (the number DELETED), not the retained-entry count (2). Pins the archive
    // edit's `files_processed` semantics against the "Delete complete: 2 files" bug.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_multi_zip(
        &path,
        &[("drop.txt", b"drop"), ("keep1.txt", b"one"), ("keep2.txt", b"two")],
    );

    let events = Arc::new(CollectorEventSink::new());
    let parent = unique_lane_id();
    installed()
        .route_archive_delete(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            &[path.join("drop.txt")],
            &parent,
            0,
            None,
        )
        .await
        .expect("start delete");

    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(
        complete[0].files_processed, 1,
        "a one-file delete must report 1 processed file, not the retained-entry count"
    );
}

#[tokio::test]
async fn a_missing_archive_emits_a_write_error_not_a_panic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("ghost.zip"); // never created

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: unique_lane_id(),
        changeset: Changeset {
            mkdirs: vec!["dir".to_string()],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        skipped_count: 0,
        preview_id: None,
    };

    installed()
        .archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");

    wait_until_async(Duration::from_secs(5), "the write-error event", || {
        !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on failure"
    );
    // Settle still fires (torn-down cleanly, no hang).
    wait_until_async(Duration::from_secs(5), "the write-settled event", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
}

/// A pane open inside a zip shows the edit as soon as it completes. The FE's
/// post-op `refresh_listing` skips an archive listing (its volume claims a
/// watch), and the FSEvents content watch is too slow and lossy to carry the
/// app's own edit, so the edit refreshes the open listings itself, before the
/// terminal event (vdavid/cmdr-reports#55: a copy into a zip, then an F8 inside
/// it, each left the pane stale until re-entered). No watch runs in this test.
mod an_open_listing_inside_the_zip {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;
    use cmdr_listing::testing::TestListing;

    /// Registers a local parent drive, writes the zip on it, and opens a listing
    /// at the zip's root showing `entries`.
    struct OpenZip {
        _tmp: tempfile::TempDir,
        path: PathBuf,
        parent: String,
        listing: cmdr_listing::testing::TestListingGuard,
    }

    impl OpenZip {
        async fn new(tag: &str, entries: &[(&str, &[u8])]) -> Self {
            let tmp = tempfile::tempdir().expect("tempdir");
            let path = tmp.path().join("a.zip");
            write_multi_zip(&path, entries);
            let parent = unique_lane_id();
            get_volume_manager().register(&parent, Arc::new(InMemoryVolume::new("parent").with_local_fs_access()));
            assert!(get_volume_manager().resolve(&parent, &path).await.is_routed());
            let names: Vec<_> = entries
                .iter()
                .map(|(name, _)| {
                    cmdr_fs::entry::FileEntry::new(
                        (*name).to_string(),
                        path.join(name).to_string_lossy().into_owned(),
                        false,
                        false,
                    )
                })
                .collect();
            let listing = TestListing::new()
                .volume(&parent)
                .path(&path)
                .entries(names)
                .insert(crate::listing_host::listings(), tag);
            Self {
                _tmp: tmp,
                path,
                parent,
                listing,
            }
        }

        fn names(&self) -> Vec<String> {
            let mut names = self.listing.entry_names();
            names.sort();
            names
        }
    }

    impl Drop for OpenZip {
        fn drop(&mut self) {
            get_volume_manager().unregister(&self.parent);
        }
    }

    async fn wait_for_complete(events: &CollectorEventSink) {
        wait_until_async(Duration::from_secs(5), "the write-complete event", || {
            !events.complete.lock_ignore_poison().is_empty()
        })
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shows_a_file_copied_in() {
        let zip = OpenZip::new("archive-edit-refresh-add", &[("keep.txt", b"keep")]).await;
        let events = Arc::new(CollectorEventSink::new());
        let request = ArchiveEditRequest {
            archive_path: zip.path.clone(),
            parent_volume_id: zip.parent.clone(),
            changeset: Changeset {
                adds: vec![AddEntry {
                    inner_path: "added.txt".to_string(),
                    source: AddSource::Bytes(b"new bytes".to_vec()),
                }],
                ..Default::default()
            },
            summary: OperationSummaryText::default(),
            skipped_count: 0,
            preview_id: None,
        };

        installed()
            .archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
            .await
            .expect("start archive edit");
        wait_for_complete(&events).await;

        assert_eq!(zip.names(), vec!["added.txt", "keep.txt"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drops_a_file_deleted_inside_it() {
        let zip = OpenZip::new(
            "archive-edit-refresh-delete",
            &[("keep.txt", b"keep"), ("drop.txt", b"drop")],
        )
        .await;
        let events = Arc::new(CollectorEventSink::new());

        installed()
            .route_archive_delete(
                Arc::clone(&events) as Arc<dyn OperationEventSink>,
                &[zip.path.join("drop.txt")],
                &zip.parent,
                0,
                None,
            )
            .await
            .expect("start delete");
        wait_for_complete(&events).await;

        assert_eq!(zip.names(), vec!["keep.txt"]);
    }
}
