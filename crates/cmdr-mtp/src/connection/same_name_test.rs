//! Two siblings answering to one name.
//!
//! PTP lets a phone or camera hold two files with the same name in one folder,
//! and both rows list with the same path. ❗ Regression anchor for #523: the
//! path cache kept the LAST twin, so a backup read the second twin twice and
//! never the first, and deleting row one deleted the other file. A name more
//! than one sibling answers to is refused now, by every path-addressed op.
//!
//! The virtual device is backed by a real folder, which can't hold twins, so
//! each cell hands the twin listing to the cache itself.

use super::testing::test_connection_manager as connection_manager;
use super::{MtpConnectionError, MtpDeleteScope};
use crate::testing::{connect_virtual_device, device_lock};
use std::path::{Path, PathBuf};

const NOTES: &str = "Meeting notes: discuss MTP E2E test strategy.\n";
const REPORT: &str = "Quarterly report: Q4 2025 placeholder content.\n";

fn is_ambiguous_at<T>(outcome: &Result<T, MtpConnectionError>, at: &str) -> bool {
    matches!(outcome, Err(MtpConnectionError::AmbiguousName { path, .. }) if path == at)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_two_siblings_answer_to_is_refused_rather_than_resolved_to_one() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(connection_manager()).await;
    let manager = connection_manager();
    manager
        .list_directory(&device.id, device.storage_id, "/Documents")
        .await
        .expect("list Documents should succeed");
    let notes = manager
        .cached_handle_for_path(&device.id, device.storage_id, Path::new("/Documents/notes.txt"))
        .await
        .expect("the listing cached notes.txt");
    let report = manager
        .cached_handle_for_path(&device.id, device.storage_id, Path::new("/Documents/report.txt"))
        .await
        .expect("the listing cached report.txt");
    // The device lists both objects as `notes.txt`.
    let twin = PathBuf::from("/Documents/notes.txt");
    manager
        .record_listing_for_test(
            &device.id,
            device.storage_id,
            Path::new("/Documents"),
            vec![(twin.clone(), notes), (twin, report)],
        )
        .await;

    let read = manager
        .open_read_session(&device.id, device.storage_id, "/Documents/notes.txt", 0, 64 * 1024)
        .await;
    assert!(
        is_ambiguous_at(&read, "/Documents/notes.txt"),
        "a copy can't pick a twin to read, got {:?}",
        read.err()
    );
    let deleted = manager
        .delete_object(
            &device.id,
            device.storage_id,
            "/Documents/notes.txt",
            MtpDeleteScope::SingleNode,
        )
        .await;
    assert!(
        is_ambiguous_at(&deleted, "/Documents/notes.txt"),
        "a delete can't pick a twin, got {deleted:?}"
    );
    let on_disk = |rel: &str| std::fs::read_to_string(device.root().join("internal").join(rel)).ok();
    assert_eq!(on_disk("Documents/notes.txt").as_deref(), Some(NOTES));
    assert_eq!(on_disk("Documents/report.txt").as_deref(), Some(REPORT));

    device.teardown(connection_manager()).await;
}
