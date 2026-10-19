//! A permanent delete of a folder that holds a mounted drive refuses up front,
//! on a synthetic APFS image mounted inside a test folder (macOS).
//!
//! `#[ignore]`d: each attaches a real disk image. Hand-run with
//! `cargo nextest run -p cmdr-operations --run-ignored only -E 'test(delete::real_image::)'`,
//! or through `pnpm check disk-images`. Serialized in the `disk-image` nextest
//! group, and machine-wide by the harness's session lock. The fixture:
//! `mount_boundary::test_support::NestedMount`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;

use super::volume_walker::delete_volume_files_with_progress_inner;
use super::walker::delete_files_with_progress_inner;
use crate::Operations;
use crate::event_sinks::{CollectorEventSink, CollectorScanPreviewSink, ScanPreviewEventSink};
use crate::mount_boundary::test_support::{NestedMount, assert_refused_at_the_mount};
use crate::scan_cache::ScanPreviewState;
use crate::scan_preview::run_scan_preview;
use crate::scan_watchdog::{ScanTarget, ScanWatchdog};
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::types::{ScanPreviewPurpose, WriteOperationConfig};

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

#[test]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
fn a_permanent_delete_of_a_folder_holding_a_mounted_drive_refuses_and_deletes_nothing() {
    let nested = NestedMount::mount();
    let engine = TestOperations::new();

    for source in [nested.folder(), nested.folder().join("mnt")] {
        let result = delete_files_with_progress_inner(
            &CollectorEventSink::new(),
            "op-delete-nested-mount",
            &state(&engine),
            std::slice::from_ref(&source),
            &WriteOperationConfig::default(),
        );
        assert_refused_at_the_mount(result);
        nested.assert_untouched();
    }
}

/// The delete reuses its preview's file list wholesale, so the preview's walk
/// is a gate too: a list that crossed the mount would be deleted as is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_delete_preview_of_a_folder_holding_a_mounted_drive_caches_nothing_to_delete() {
    let nested = NestedMount::mount();
    let engine = TestOperations::new();
    let sources = vec![nested.folder()];
    let preview_id = format!("nested-mount-{}", std::process::id());
    let preview_state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(50),
    });
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&sources, "root"),
        Duration::from_secs(600),
        Arc::clone(&preview_state),
        Arc::new(CollectorScanPreviewSink::new()) as Arc<dyn ScanPreviewEventSink>,
    );
    let (walker, walk_id, walk_sources) = (Operations::clone(&engine), preview_id.clone(), sources.clone());
    tokio::task::spawn_blocking(move || {
        run_scan_preview(
            &walker,
            Arc::new(CollectorScanPreviewSink::new()),
            walk_id,
            walk_sources,
            preview_state,
            ScanPreviewPurpose::Delete,
            watchdog,
        );
    })
    .await
    .expect("the walk thread joins");

    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&preview_id, &sources)
            .is_none(),
        "❗ a delete preview must not cache a file list that crosses the mount"
    );
    let result = delete_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-delete-nested-mount-preview",
        &state(&engine),
        &sources,
        &WriteOperationConfig {
            preview_id: Some(preview_id),
            ..WriteOperationConfig::default()
        },
    );
    assert_refused_at_the_mount(result);
    nested.assert_untouched();
}

/// A folder opened as its own volume (`LocalPosixVolume`) reaches the mount
/// through the volume walker, which asks no device of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "attaches a real disk image; run via `pnpm check disk-images`"]
async fn a_volume_delete_of_a_folder_holding_a_mounted_drive_refuses_and_deletes_nothing() {
    let nested = NestedMount::mount();
    let engine = TestOperations::new();
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "V",
        nested.dir.to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    engine.volumes().register("vol-nested-mount", Arc::clone(&volume));

    let result = delete_volume_files_with_progress_inner(
        volume,
        "vol-nested-mount",
        &CollectorEventSink::new(),
        "op-volume-delete-nested-mount",
        &state(&engine),
        &[PathBuf::from("folder")],
        &WriteOperationConfig::default(),
    )
    .await;

    assert_refused_at_the_mount(result);
    nested.assert_untouched();
}
