//! Two items with one name in a phone or camera folder, copied off through the
//! app's whole pipeline (#523). ❗ Regression anchor: both rows resolved to the
//! last twin, so a backup read it twice and never the first. Each row is its
//! own object now, and the second lands on a taken name, so it goes through the
//! conflict policy like any other clash.
//!
//! `make_twins` has `report.txt` answer to `notes.txt` too (the virtual device's
//! real folder can't hold twins), so each twin's bytes tell which one landed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;
use cmdr_operations::{ConflictResolution, VolumeCopyConfig};

use super::network_transfer_test_support::start_copy_on;
use crate::mtp::test_support::{connect_virtual_device, device_lock, make_twins, teardown, volume_for};
use crate::operations_host::app_engine;
use crate::test_support::TestDir;

const NOTES: &str = "Meeting notes: discuss MTP E2E test strategy.\n";
const REPORT: &str = "Quarterly report: Q4 2025 placeholder content.\n";

/// What every file in `dir` whose name starts with `notes` holds.
fn notes_landed_in(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable entry").path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("notes"))
        })
        .map(|path| std::fs::read_to_string(&path).expect("a landed copy is readable"))
        .collect()
}

/// Copies `sources` off the device into a fresh local folder, renaming on a
/// clash, and answers that folder.
async fn copy_off(label: &str, device_volume: Arc<dyn Volume>, sources: Vec<PathBuf>) -> TestDir {
    let local_dir = TestDir::new(label);
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*local_dir,
        crate::volume_host::host(),
    ));
    let running = start_copy_on(
        app_engine(),
        label,
        device_volume,
        sources,
        local,
        PathBuf::from(""),
        VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Rename,
            ..VolumeCopyConfig::default()
        },
    )
    .await;
    running.settle().await;
    running.assert_no_errors();
    local_dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copying_a_folder_holding_twins_off_a_device_keeps_both() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, None).await);
    make_twins(&device, "/Documents", "notes.txt", "report.txt").await;

    let local_dir = copy_off("mtp_twins_folder_off", volume, vec![PathBuf::from("/Documents")]).await;

    assert_eq!(
        notes_landed_in(&local_dir.join("Documents")),
        BTreeSet::from([NOTES.to_string(), REPORT.to_string()]),
        "each twin lands once, the second under a free name"
    );

    teardown(device).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copying_both_selected_twin_rows_off_a_device_keeps_both() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let mtp_volume = volume_for(&device, None).await;
    make_twins(&device, "/Documents", "notes.txt", "report.txt").await;
    let rows: Vec<PathBuf> = mtp_volume
        .list_directory(Path::new("/Documents"), None)
        .await
        .expect("listing Documents should succeed")
        .into_iter()
        .filter(|entry| entry.name == "notes.txt")
        .map(|entry| PathBuf::from(entry.path))
        .collect();
    assert_eq!(rows.len(), 2, "both twins list");

    let local_dir = copy_off("mtp_twins_rows_off", Arc::new(mtp_volume), rows).await;

    assert_eq!(
        notes_landed_in(&local_dir),
        BTreeSet::from([NOTES.to_string(), REPORT.to_string()]),
        "each twin lands once, the second under a free name"
    );

    teardown(device).await;
}
