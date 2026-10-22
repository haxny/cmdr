//! Data-safety cells for the NFS backend, on the knfsd fixture's `/insecure`
//! export: a failed merge that must not sweep the user's folder, a failed move
//! that must lose no byte, a delete bound to its preview, a recursive delete
//! that takes exactly the selection, an unanswerable source probe, a download
//! stopped mid-file, and a name taken mid-upload.
//!
//! The scenarios are backend-blind and live in `network_safety_test_support.rs`;
//! the cells stay here because the integration lane selects them by the
//! `nfs_integration_` name prefix.

use super::network_safety_test_support::{
    STAGED_ON_EVERY_BACKEND, a_cancelled_download_leaves_nothing_behind,
    a_delete_of_a_non_empty_folder_takes_exactly_the_selection,
    a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree,
    a_failed_folder_copy_onto_a_users_folder_keeps_their_files, a_failed_folder_move_onto_a_users_folder_loses_no_byte,
    a_name_taken_mid_upload_is_never_replaced, an_unknown_source_type_never_clears_a_server_folder,
};
use super::nfs_test_support::fixture;
use cmdr_operations::ConflictResolution;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_name_taken_mid_upload_is_never_replaced() {
    let (remote, dir) = fixture("name-taken-mid-upload").await;
    a_name_taken_mid_upload_is_never_replaced(remote, dir, ConflictResolution::Skip, STAGED_ON_EVERY_BACKEND).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_failed_folder_copy_onto_a_users_folder_keeps_their_files() {
    let (remote, dir) = fixture("failed-merge-copy").await;
    a_failed_folder_copy_onto_a_users_folder_keeps_their_files(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_failed_folder_move_onto_a_users_folder_loses_no_byte() {
    let (remote, dir) = fixture("failed-merge-move").await;
    a_failed_folder_move_onto_a_users_folder_loses_no_byte(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree() {
    let (remote, dir) = fixture("delete-preview").await;
    a_delete_with_a_local_shaped_preview_removes_only_the_requested_tree(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_delete_of_a_non_empty_folder_takes_exactly_the_selection() {
    let (remote, dir) = fixture("delete-tree").await;
    a_delete_of_a_non_empty_folder_takes_exactly_the_selection(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_an_unknown_source_type_never_clears_a_server_folder() {
    let (remote, dir) = fixture("unknown-source-type").await;
    an_unknown_source_type_never_clears_a_server_folder(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_cancelled_download_leaves_nothing_behind() {
    let (remote, dir) = fixture("cancelled-download").await;
    a_cancelled_download_leaves_nothing_behind(remote, dir).await;
}
