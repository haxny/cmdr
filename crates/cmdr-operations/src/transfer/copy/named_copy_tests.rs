//! Named local copies: landing, conflicts, and source preservation. A leaf the
//! engine must never see is refused at the start: `../../named_leaf_entry_tests.rs`.

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;

fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(200)))
}

#[test]
fn named_copy_lands_beside_source_without_changing_it() {
    let engine = TestOperations::new();
    let tmp = cmdr_fs::testing::TestDir::new("named_copy");
    let source = tmp.as_ref().join("original.txt");
    fs::write(&source, b"keep the original").unwrap();
    let events = CollectorEventSink::new();
    let config = WriteOperationConfig {
        leaves: crate::types::TransferLeaves::single("backup.txt"),
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &events,
        "named-copy",
        &make_state(&engine),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"keep the original");
    assert_eq!(fs::read(tmp.as_ref().join("backup.txt")).unwrap(), b"keep the original");
    assert!(!tmp.as_ref().join("original (1).txt").exists());
}

#[test]
fn named_folder_copy_maps_children_and_empty_directories() {
    let engine = TestOperations::new();
    let tmp = cmdr_fs::testing::TestDir::new("named_folder_copy");
    let source = tmp.as_ref().join("original");
    fs::create_dir_all(source.join("empty/nested")).unwrap();
    fs::write(source.join("notes.txt"), b"notes").unwrap();
    let config = WriteOperationConfig {
        leaves: crate::types::TransferLeaves::single("backup"),
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "named-folder-copy",
        &make_state(&engine),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(tmp.as_ref().join("backup/notes.txt")).unwrap(), b"notes");
    assert!(tmp.as_ref().join("backup/empty/nested").is_dir());
    assert!(source.join("empty/nested").is_dir());
}

#[test]
fn named_copy_honors_skip_and_overwrite_at_the_requested_name() {
    let engine = TestOperations::new();
    for resolution in [ConflictResolution::Skip, ConflictResolution::Overwrite] {
        let tmp = cmdr_fs::testing::TestDir::new("named_copy_conflict");
        let source = tmp.as_ref().join("original.txt");
        let target = tmp.as_ref().join("backup.txt");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"existing").unwrap();
        let config = WriteOperationConfig {
            leaves: crate::types::TransferLeaves::single("backup.txt"),
            conflict_resolution: resolution,
            ..Default::default()
        };
        copy_files_with_progress_inner(
            &CollectorEventSink::new(),
            "named-copy-conflict",
            &make_state(&engine),
            std::slice::from_ref(&source),
            tmp.as_ref(),
            &config,
        )
        .unwrap();
        let expected: &[u8] = if resolution == ConflictResolution::Skip {
            b"existing"
        } else {
            b"source"
        };
        assert_eq!(fs::read(&target).unwrap(), expected);
        assert_eq!(fs::read(&source).unwrap(), b"source");
    }
}

#[test]
fn named_copy_of_itself_duplicates_instead_of_overwriting() {
    let engine = TestOperations::new();
    let tmp = cmdr_fs::testing::TestDir::new("named_copy_self");
    let source = tmp.as_ref().join("original.txt");
    fs::write(&source, b"source").unwrap();
    let config = WriteOperationConfig {
        leaves: crate::types::TransferLeaves::single("original.txt"),
        conflict_resolution: ConflictResolution::Overwrite,
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "named-copy-self",
        &make_state(&engine),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(tmp.as_ref().join("original (1).txt")).unwrap(), b"source");
}
