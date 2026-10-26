//! An explicit transfer leaf (`transfer/DETAILS.md` § Named destinations) is
//! refused at the routed start, before anything is admitted or written: a leaf
//! that isn't one plain component, a leaf for more than one source, and a leaf
//! too long for the filesystem.

use std::path::PathBuf;
use std::sync::Arc;

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::types::{VolumeCopyConfig, WriteOperationError};
use crate::testing::TestOperations;
use cmdr_fs::testing::TestDir;
use cmdr_operation_log::types::Initiator;

/// Starts a routed copy or move on the local `root` volume, as the dialog does.
async fn start(
    engine: &TestOperations,
    is_move: bool,
    sources: Vec<PathBuf>,
    destination: &std::path::Path,
    leaf: &str,
) -> Result<super::types::WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        destination_name: Some(leaf.to_string()),
        ..VolumeCopyConfig::default()
    };
    let dest = destination.display().to_string();
    if is_move {
        engine
            .start_volume_move(
                events,
                "root".into(),
                sources,
                "root".into(),
                dest,
                config,
                Initiator::User,
                None,
            )
            .await
    } else {
        engine
            .start_volume_copy(
                events,
                "root".into(),
                sources,
                "root".into(),
                dest,
                config,
                Initiator::User,
                None,
            )
            .await
    }
}

fn fixture(tag: &str) -> (TestDir, PathBuf, PathBuf) {
    let tmp = TestDir::new(tag);
    let source = tmp.join("original.txt");
    std::fs::write(&source, b"payload").expect("source");
    let dest = tmp.join("dest");
    std::fs::create_dir(&dest).expect("dest");
    (tmp, source, dest)
}

fn assert_untouched(source: &std::path::Path, dest: &std::path::Path) {
    assert_eq!(std::fs::read(source).expect("the source stays"), b"payload");
    assert_eq!(std::fs::read_dir(dest).expect("dest").count(), 0, "nothing landed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leaf_that_is_not_one_plain_component_refuses_at_the_start() {
    let engine = TestOperations::new();
    engine.register_local_root();
    for is_move in [false, true] {
        for leaf in ["", ".", "..", "../escape.txt", "/absolute", "bad\0name"] {
            let (_tmp, source, dest) = fixture("named_leaf_entry_bad");
            let result = start(&engine, is_move, vec![source.clone()], &dest, leaf).await;
            assert!(
                matches!(result, Err(WriteOperationError::InvalidName { .. })),
                "move={is_move} leaf={leaf:?}: {result:?}"
            );
            assert_untouched(&source, &dest);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leaf_for_more_than_one_source_refuses_at_the_start() {
    let engine = TestOperations::new();
    engine.register_local_root();
    for is_move in [false, true] {
        let (tmp, source, dest) = fixture("named_leaf_entry_batch");
        let other = tmp.join("other.txt");
        std::fs::write(&other, b"other").expect("other");
        let result = start(&engine, is_move, vec![source.clone(), other], &dest, "backup.txt").await;
        assert!(
            matches!(result, Err(WriteOperationError::InvalidName { .. })),
            "move={is_move}: {result:?}"
        );
        assert_untouched(&source, &dest);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leaf_too_long_for_the_filesystem_refuses_at_the_start() {
    let engine = TestOperations::new();
    engine.register_local_root();
    for is_move in [false, true] {
        let (_tmp, source, dest) = fixture("named_leaf_entry_long");
        let result = start(&engine, is_move, vec![source.clone()], &dest, &"n".repeat(256)).await;
        assert!(
            matches!(result, Err(WriteOperationError::IoError { .. })),
            "move={is_move}: {result:?}"
        );
        assert_untouched(&source, &dest);
    }
}
