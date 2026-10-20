//! A source FOLDER meeting an archive FILE of the same name: the type-crossing
//! clash the file-over-folder direction already guards. The zip can't hold
//! both (every reader drops an entry whose parent is a file), so a folder only
//! lands there when a person answered Overwrite for that clash; a blanket
//! policy skips it, and a skip keeps a move's source.

use super::test_support::*;

/// Runs a copy/move INTO `archive` of local dir `src_rel` under `policy`,
/// landing at the archive root, and waits for the terminal complete event.
async fn run_into(
    engine: &TestOperations,
    archive: &Path,
    src_root: &Path,
    src_rel: &str,
    policy: ConflictResolution,
    is_move: bool,
) -> Arc<CollectorEventSink> {
    use cmdr_local::LocalPosixVolume;

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "src",
        src_root.to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let events = Arc::new(CollectorEventSink::new());
    engine
        .route_archive_copy_into(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            source_volume,
            vec![PathBuf::from(src_rel)],
            archive.to_path_buf(),
            unique_lane_id(),
            policy,
            0,
            is_move,
            None,
            None,
        )
        .await
        .expect("start copy-into");
    events
}

async fn wait_complete(events: &CollectorEventSink) -> u64 {
    wait_until_async(Duration::from_secs(5), "the write-complete event", || {
        !events.complete.lock_ignore_poison().is_empty()
    })
    .await;
    events.complete.lock_ignore_poison()[0].files_skipped as u64
}

/// A source folder `notes/` with two files, next to `src_root`.
fn notes_folder(src_root: &Path) {
    std::fs::create_dir_all(src_root.join("notes")).expect("mkdir src");
    std::fs::write(src_root.join("notes/a.txt"), b"aaa").expect("w a");
    std::fs::write(src_root.join("notes/b.txt"), b"bbb").expect("w b");
}

/// Every blanket policy refuses the folder: the archive keeps its file, gains
/// nothing under the file's name, counts the skip, and a move keeps the source.
#[tokio::test]
async fn moving_a_folder_onto_an_archive_file_of_its_name_skips_it_and_keeps_the_source() {
    let engine = TestOperations::new();
    for policy in [
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::OverwriteSmaller,
        ConflictResolution::OverwriteOlder,
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive = tmp.path().join("a.zip");
        write_multi_zip(&archive, &[("notes", b"the file"), ("keep.txt", b"k")]);
        let src_root = tmp.path().join("src");
        notes_folder(&src_root);

        let events = run_into(&engine, &archive, &src_root, "notes", policy, true).await;
        let skipped = wait_complete(&events).await;

        assert_eq!(
            read_entry(&archive, "notes").as_deref(),
            Some(b"the file".as_slice()),
            "{policy:?}"
        );
        for hidden in ["notes/", "notes/a.txt", "notes/b.txt"] {
            assert_eq!(
                read_entry(&archive, hidden),
                None,
                "{policy:?} must not add {hidden} under a file"
            );
        }
        assert!(skipped >= 1, "{policy:?} must count the refused folder as skipped");
        assert!(
            src_root.join("notes/a.txt").exists() && src_root.join("notes/b.txt").exists(),
            "{policy:?}: a move whose folder was refused must keep its source"
        );
    }
}

/// The same clash deep in the moved tree: `deep/x` is a folder in the source and
/// a file in the archive. The rest of the tree lands; `x` and its source stay.
#[tokio::test]
async fn a_nested_folder_meeting_an_archive_file_is_skipped_and_the_rest_lands() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("deep/x", b"the file")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("deep/x/sub")).expect("mkdir src");
    std::fs::write(src_root.join("deep/x/sub/inner.txt"), b"inner").expect("w inner");
    std::fs::write(src_root.join("deep/other.txt"), b"other").expect("w other");

    let events = run_into(
        &engine,
        &archive,
        &src_root,
        "deep",
        ConflictResolution::Overwrite,
        true,
    )
    .await;
    let skipped = wait_complete(&events).await;

    assert_eq!(read_entry(&archive, "deep/x").as_deref(), Some(b"the file".as_slice()));
    assert_eq!(read_entry(&archive, "deep/x/sub/inner.txt"), None);
    assert_eq!(
        read_entry(&archive, "deep/other.txt").as_deref(),
        Some(b"other".as_slice())
    );
    assert!(skipped >= 1, "the refused nested folder counts as skipped");
    assert!(
        src_root.join("deep/x/sub/inner.txt").exists(),
        "a move with a refused folder keeps its source"
    );
}

/// Rename gives the folder a free name, and everything under it follows; the
/// move is clean, so its source goes.
#[tokio::test]
async fn rename_lands_a_folder_beside_an_archive_file_of_its_name() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("notes", b"the file")]);
    let src_root = tmp.path().join("src");
    notes_folder(&src_root);

    let events = run_into(&engine, &archive, &src_root, "notes", ConflictResolution::Rename, true).await;
    let skipped = wait_complete(&events).await;

    assert_eq!(read_entry(&archive, "notes").as_deref(), Some(b"the file".as_slice()));
    assert_eq!(
        read_entry(&archive, "notes (1)/a.txt").as_deref(),
        Some(b"aaa".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "notes (1)/b.txt").as_deref(),
        Some(b"bbb".as_slice())
    );
    assert_eq!(skipped, 0);
    wait_until_async(Duration::from_secs(5), "the moved source to go", || {
        !src_root.join("notes").exists()
    })
    .await;
}

/// Stop asks about the folder itself, naming both kinds, and an Overwrite
/// answered there replaces the file with the folder.
#[tokio::test]
async fn stop_prompts_for_the_folder_and_an_answered_overwrite_replaces_the_file() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("notes", b"the file")]);
    let src_root = tmp.path().join("src");
    notes_folder(&src_root);

    let events = run_into(&engine, &archive, &src_root, "notes", ConflictResolution::Stop, false).await;
    wait_until_async(Duration::from_secs(5), "the write-conflict event", || {
        !events.conflicts.lock_ignore_poison().is_empty()
    })
    .await;
    let (op_id, conflict_id) = {
        let conflicts = events.conflicts.lock_ignore_poison();
        let conflict = &conflicts[0];
        assert!(conflict.source_is_directory, "the prompt names the incoming folder");
        assert!(!conflict.destination_is_directory, "and the archive file it meets");
        (conflict.operation_id.clone(), conflict.conflict_id)
    };
    engine.resolve_write_conflict(&op_id, conflict_id, ConflictResolution::Overwrite, false);
    let skipped = wait_complete(&events).await;

    assert_eq!(
        events.conflicts.lock_ignore_poison().len(),
        1,
        "one prompt, for the folder"
    );
    assert_eq!(
        read_entry(&archive, "notes"),
        None,
        "the answered Overwrite replaced the file"
    );
    assert_eq!(read_entry(&archive, "notes/a.txt").as_deref(), Some(b"aaa".as_slice()));
    assert_eq!(read_entry(&archive, "notes/b.txt").as_deref(), Some(b"bbb".as_slice()));
    assert_eq!(skipped, 0);
}
