//! What a transfer MEANS on the knfsd fixture's `/insecure` export: merges,
//! policies, moves in both directions, and same-export moves and copies.
//!
//! The backend-blind scenarios live in `network_semantics_test_support.rs` and
//! `network_move_drift_test_support.rs`, shared with the other server
//! backends; the cells stay here because the integration lane selects them by
//! the `nfs_integration_` name prefix. Not here: the link-move cells (this
//! backend makes no links) and the dialog-address cells, which pin SFTP's and
//! SMB's own app roots.

use super::network_move_drift_test_support::{
    a_file_added_mid_move_off_the_server_stays, a_file_saved_over_mid_move_off_the_server_stays,
};
use super::network_semantics_test_support::{
    a_copy_into_a_missing_nested_destination_makes_every_level,
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash,
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file, a_folder_moved_off_the_server_leaves_no_source,
    a_folder_moved_onto_the_server_leaves_no_source, a_move_merge_onto_the_server_spares_what_it_skipped,
    a_multi_megabyte_file_round_trips_byte_exact, a_rename_policy_lands_the_clash_beside_the_users_file,
    a_same_server_copy_duplicates_a_tree, a_same_server_move_merges_without_a_folder_prompt,
    a_same_server_move_without_a_clash_moves_the_folder_whole, many_files_at_full_concurrency_land_intact,
    overwrite_smaller_replaces_only_the_smaller_destination,
};
use super::nfs_test_support::fixture;

// ── Merges and policies ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_deep_clash_merge_under_skip_keeps_every_dest_only_file() {
    let (remote, dir) = fixture("merge-skip").await;
    a_deep_clash_merge_under_skip_keeps_every_dest_only_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_deep_clash_merge_under_overwrite_replaces_only_the_clash() {
    let (remote, dir) = fixture("merge-overwrite").await;
    a_deep_clash_merge_under_overwrite_replaces_only_the_clash(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_rename_policy_lands_the_clash_beside_the_users_file() {
    let (remote, dir) = fixture("merge-rename").await;
    a_rename_policy_lands_the_clash_beside_the_users_file(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_overwrite_smaller_replaces_only_the_smaller_destination() {
    let (remote, dir) = fixture("overwrite-smaller").await;
    overwrite_smaller_replaces_only_the_smaller_destination(remote, dir).await;
}

// ── Moves across the volume boundary ──────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_move_merge_onto_the_server_spares_what_it_skipped() {
    let (remote, dir) = fixture("move-merge-skip").await;
    a_move_merge_onto_the_server_spares_what_it_skipped(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_folder_moved_onto_the_server_leaves_no_source() {
    let (remote, dir) = fixture("move-onto").await;
    a_folder_moved_onto_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_folder_moved_off_the_server_leaves_no_source() {
    let (remote, dir) = fixture("move-off").await;
    a_folder_moved_off_the_server_leaves_no_source(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_file_saved_over_mid_move_off_the_server_stays() {
    let (remote, dir) = fixture("drift-saved-over").await;
    a_file_saved_over_mid_move_off_the_server_stays(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_file_added_mid_move_off_the_server_stays() {
    let (remote, dir) = fixture("drift-added").await;
    a_file_added_mid_move_off_the_server_stays(remote, dir).await;
}

// ── Staying on the export ─────────────────────────────────────────────

/// Every rename here is the claim then `RENAME`, since v3 `RENAME` replaces.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_same_server_move_merges_without_a_folder_prompt() {
    let (remote, dir) = fixture("same-move-merge").await;
    a_same_server_move_merges_without_a_folder_prompt(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_same_server_move_without_a_clash_moves_the_folder_whole() {
    let (remote, dir) = fixture("same-move").await;
    a_same_server_move_without_a_clash_moves_the_folder_whole(remote, dir).await;
}

/// v3 has no server-side copy, so the same copy streams through the client.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_same_server_copy_duplicates_a_tree() {
    let (remote, dir) = fixture("same-copy").await;
    a_same_server_copy_duplicates_a_tree(remote, dir).await;
}

// ── Destinations, sizes, and load ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_copy_into_a_missing_nested_destination_makes_every_level() {
    let (remote, dir) = fixture("missing-nested").await;
    a_copy_into_a_missing_nested_destination_makes_every_level(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_a_multi_megabyte_file_round_trips_byte_exact() {
    let (remote, dir) = fixture("multi-megabyte").await;
    a_multi_megabyte_file_round_trips_byte_exact(remote, dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
async fn nfs_integration_many_files_at_full_concurrency_land_intact() {
    let (remote, dir) = fixture("many-files").await;
    many_files_at_full_concurrency_land_intact(remote, dir).await;
}
