//! Hardlink-aware progress accounting for copy and move.
//!
//! Same invariant as the delete-side test in
//! `../delete/hardlink_progress_tests.rs`: `bytes_done` reported during the
//! active phase must not exceed `bytes_total` reported during the scan.
//! Before this work the copy walker summed `metadata.len()` per `FileInfo`
//! while scan dedup'd hardlinks by inode, so the bar overshot on hardlink-
//! heavy trees the same way delete did. See `copy.rs::copy_single_item` and
//! `move_op.rs::move_with_staging` for the active sites.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::event_sinks::CollectorEventSink;
use super::super::types::WriteOperationConfig;
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;
use crate::testing::TestOperations;
use cmdr_fs::testing::TestDir;

fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("hardlink_xfer_{name}"))
}

/// Build a tree under `root` containing one inode shared by three hardlinks
/// plus one standalone file. Returns the source directory the test should
/// copy/move. Sizes are chosen so the overshoot would be unambiguous:
/// un-dedup'd numerator = 3 * 1024 + 4096 = 7168; dedup'd = 1024 + 4096 = 5120.
fn build_hardlink_tree(root: &Path, subdir: &str) -> PathBuf {
    let src = root.join(subdir);
    fs::create_dir_all(&src).unwrap();
    let payload = vec![0u8; 1024];
    let original = src.join("original");
    fs::write(&original, &payload).unwrap();
    fs::hard_link(&original, src.join("hardlink_a")).unwrap();
    fs::hard_link(&original, src.join("hardlink_b")).unwrap();
    fs::write(src.join("standalone"), vec![0u8; 4096]).unwrap();
    src
}

fn copying_phase_bytes_total(sink: &CollectorEventSink) -> u64 {
    sink.progress
        .lock()
        .unwrap()
        .iter()
        .find(|e| matches!(e.phase, super::super::types::WriteOperationPhase::Copying))
        .map(|e| e.bytes_total)
        .expect("at least one Copying-phase progress event")
}

/// Active-phase events only: a mid-scan event carries the bytes FOUND so far against a
/// `bytes_total` of 0, since the total isn't known until the walk ends.
fn assert_no_mid_flight_overshoot(sink: &CollectorEventSink) {
    let progress = sink.progress.lock().unwrap();
    let active = progress
        .iter()
        .filter(|e| !matches!(e.phase, super::super::types::WriteOperationPhase::Scanning));
    for event in active {
        assert!(
            event.bytes_done <= event.bytes_total,
            "mid-flight progress event overshoots: bytes_done={} > bytes_total={} (phase={:?})",
            event.bytes_done,
            event.bytes_total,
            event.phase
        );
    }
}

/// Copy counts the **write footprint** (every hardlink at full size), because
/// a copy materializes each link as an independent file — that's the bytes it
/// actually writes and the bar it fills. The dedup'd `du`-size is surfaced
/// separately to the dialog as context (see the scan-preview path). So both
/// the Copying-phase denominator and the final `bytes_processed` are 7168
/// (3 × 1024 hardlinks + 4096 standalone), NOT the dedup'd 5120.
#[test]
fn copy_counts_write_footprint_for_hardlinks() {
    let engine = TestOperations::new();
    let root = create_temp_dir("copy_hardlinks");
    let src = build_hardlink_tree(&root, "src");
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let op_id = "copy".to_string();
    // No throttle: every progress event fires, so the overshoot check sees all of them.
    let op = engine.register_state(&op_id, Arc::new(engine.new_operation_state(Duration::ZERO)));
    let sink = CollectorEventSink::new();
    let sources = vec![src.clone()];
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(&sink, &op_id, op.state(), &sources, &dest, &config);

    assert!(result.is_ok(), "copy must succeed; got {result:?}");

    let bytes_total = copying_phase_bytes_total(&sink);
    let bytes_processed = sink
        .complete
        .lock()
        .unwrap()
        .first()
        .map(|e| e.bytes_processed)
        .expect("write-complete must fire");

    assert_eq!(
        bytes_total, 7168,
        "copy denominator should be the write footprint (every hardlink at full size)"
    );
    assert_eq!(
        bytes_processed, bytes_total,
        "copy numerator must reach the write-footprint denominator exactly"
    );
    assert_no_mid_flight_overshoot(&sink);
}

/// Cross-fs move would be the truer test for hardlink dedup (rename within
/// one FS doesn't sum bytes — `move_op.rs:222` ships `bytes_processed: 0`),
/// but cross-fs requires two real filesystems we can't reliably get inside a
/// `cargo nextest` run. Same-fs rename still validates the contract that
/// move's terminal `bytes_processed` doesn't exceed `bytes_total`.
#[test]
fn move_hardlinked_files_does_not_overshoot_progress() {
    let engine = TestOperations::new();
    let root = create_temp_dir("move_hardlinks");
    let src = build_hardlink_tree(&root, "src");
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let op_id = "move".to_string();
    // No throttle: every progress event fires, so the overshoot check sees all of them.
    let op = engine.register_state(&op_id, Arc::new(engine.new_operation_state(Duration::ZERO)));
    let sink = CollectorEventSink::new();
    let sources = vec![src.clone()];
    let config = WriteOperationConfig::default();

    let result = move_files_with_progress_inner(&sink, &op_id, op.state(), &sources, &dest, &config);

    assert!(result.is_ok(), "move must succeed; got {result:?}");
    assert_no_mid_flight_overshoot(&sink);
}
