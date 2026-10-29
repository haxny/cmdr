//! Filing onto a phone over a virtual MTP device: a copy or move that names
//! each item, then its undo (`network_named_leaves_test_support.rs`). Within
//! the device, MTP moves an object and then renames it, so the cell keeps a
//! bystander under the ORIGINAL name in the destination folder.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use super::network_named_leaves_test_support::{
    Verb, items_move_named_within_the_server_and_undo_restores_them,
    local_items_land_named_on_the_server_and_undo_restores_them,
};
use crate::mtp::test_support::{connect_virtual_device, device_lock, teardown, volume_for};

/// A scratch folder of its own on the device's writable storage.
const DIR: &str = "/Documents/named-leaves";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_naming_each_item_onto_a_phone_lands_named_and_undoes() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, None).await);
    local_items_land_named_on_the_server_and_undo_restores_them(volume, PathBuf::from(DIR), Verb::Copy).await;
    teardown(device).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_move_naming_each_item_onto_a_phone_lands_named_and_undoes() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, None).await);
    local_items_land_named_on_the_server_and_undo_restores_them(volume, PathBuf::from(DIR), Verb::Move).await;
    teardown(device).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_move_naming_each_item_within_a_phone_lands_named_and_undoes() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, None).await);
    items_move_named_within_the_server_and_undo_restores_them(volume, PathBuf::from(DIR)).await;
    teardown(device).await;
}
