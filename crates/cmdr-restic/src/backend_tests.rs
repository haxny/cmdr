use super::VolumeBackend;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use rustic_core::{FileType, Id, ReadBackend, WriteBackend};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ROOT: &str = "/backups/repo";
const PACK_A: &str = "ab00000000000000000000000000000000000000000000000000000000000001";
const PACK_B: &str = "cd00000000000000000000000000000000000000000000000000000000000002";
const SNAP: &str = "5a00000000000000000000000000000000000000000000000000000000000003";
const KEY: &str = "4b00000000000000000000000000000000000000000000000000000000000004";

fn id(hex: &str) -> Id {
    hex.parse().expect("test ids are valid hex")
}

/// A parent holding a repo's on-disk layout, with arbitrary bytes for content:
/// the adapter only moves bytes, so no real repo is needed at this layer.
async fn layout_volume(volume: InMemoryVolume) -> Arc<InMemoryVolume> {
    let files: [(String, Vec<u8>); 6] = [
        (format!("{ROOT}/config"), b"config bytes".to_vec()),
        (format!("{ROOT}/keys/{KEY}"), b"key".to_vec()),
        (format!("{ROOT}/snapshots/{SNAP}"), b"snapshot".to_vec()),
        (format!("{ROOT}/data/ab/{PACK_A}"), (0u8..=99).collect()),
        (format!("{ROOT}/data/cd/{PACK_B}"), vec![7; 1_000]),
        // Not a restic id: a Finder droppings file the listing must skip.
        (format!("{ROOT}/snapshots/.DS_Store"), b"junk".to_vec()),
    ];
    for dir in ["keys", "snapshots", "index", "data", "data/ab", "data/cd"] {
        volume
            .create_directory(&PathBuf::from(format!("{ROOT}/{dir}")))
            .await
            .expect("mkdir");
    }
    for (path, content) in &files {
        volume
            .create_file(Path::new(path), content)
            .await
            .expect("write fixture file");
    }
    Arc::new(volume)
}

fn backend(parent: &Arc<InMemoryVolume>) -> Arc<VolumeBackend> {
    let parent: Arc<dyn Volume> = Arc::clone(parent) as Arc<dyn Volume>;
    Arc::new(VolumeBackend::new(
        parent,
        PathBuf::from(ROOT),
        tokio::runtime::Handle::current(),
    ))
}

/// Runs `f` the way production does: on a blocking thread, never an async worker.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.expect("blocking task")
}

fn sorted(mut list: Vec<(Id, u32)>) -> Vec<(Id, u32)> {
    list.sort_unstable_by_key(|(id, _)| *id);
    list
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_each_type_from_its_folder_with_sizes() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let (keys, snaps, index, packs) = blocking(move || {
        (
            be.list_with_size(FileType::Key).expect("keys"),
            be.list_with_size(FileType::Snapshot).expect("snapshots"),
            be.list_with_size(FileType::Index).expect("index"),
            be.list_with_size(FileType::Pack).expect("packs"),
        )
    })
    .await;
    assert_eq!(keys, vec![(id(KEY), 3)]);
    assert_eq!(snaps, vec![(id(SNAP), 8)], "`.DS_Store` isn't an id, so it's skipped");
    assert_eq!(index, vec![]);
    assert_eq!(sorted(packs), vec![(id(PACK_A), 100), (id(PACK_B), 1_000)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_config_lists_as_one_default_id() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let config = blocking(move || be.list_with_size(FileType::Config).expect("config")).await;
    assert_eq!(config, vec![(Id::default(), 12)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_config_lists_as_none() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    parent
        .delete(Path::new(&format!("{ROOT}/config")))
        .await
        .expect("rm config");
    let be = backend(&parent);
    let config = blocking(move || be.list_with_size(FileType::Config).expect("config")).await;
    assert_eq!(config, vec![]);
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_a_whole_file_from_its_layout_path() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let (config, pack) = blocking(move || {
        (
            be.read_full(FileType::Config, &Id::default()).expect("config"),
            be.read_full(FileType::Pack, &id(PACK_B)).expect("pack"),
        )
    })
    .await;
    assert_eq!(&config[..], b"config bytes");
    assert_eq!(pack.len(), 1_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_read_is_one_ranged_read_of_exactly_that_window() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let bytes = blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 10, 5)
            .expect("read")
    })
    .await;
    assert_eq!(&bytes[..], &[10, 11, 12, 13, 14]);
    assert_eq!(parent.read_range_log(), vec![(10, 5)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_partial_read_falls_back_to_a_whole_file_read_without_ranged_reads() {
    let parent = layout_volume(InMemoryVolume::new("parent").with_read_range_unsupported()).await;
    let be = backend(&parent);
    let bytes = blocking(move || {
        be.read_partial(FileType::Pack, &id(PACK_A), false, 95, 5)
            .expect("read")
    })
    .await;
    assert_eq!(&bytes[..], &[95, 96, 97, 98, 99]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_short_partial_read_is_an_error() {
    // A pack truncated under us must fail loudly, never hand rustic fewer bytes
    // than it asked for.
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let result = blocking(move || be.read_partial(FileType::Pack, &id(PACK_A), false, 98, 5)).await;
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_parent_failure_carries_the_typed_volume_error() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let err = blocking(move || {
        be.read_full(FileType::Snapshot, &id(PACK_A))
            .expect_err("no such snapshot")
    })
    .await;
    assert!(matches!(super::volume_error_in(&*err), Some(VolumeError::NotFound(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn every_write_is_refused_and_counted() {
    let parent = layout_volume(InMemoryVolume::new("parent")).await;
    let be = backend(&parent);
    let be_for_writes = Arc::clone(&be);
    let results = blocking(move || {
        let be = be_for_writes;
        [
            be.create(),
            be.write_bytes(FileType::Snapshot, &id(SNAP), false, b"new".to_vec().into()),
            be.remove(FileType::Snapshot, &id(SNAP), false),
        ]
    })
    .await;
    assert!(results.iter().all(Result::is_err));
    assert_eq!(be.write_attempts(), 3);
    // And the parent is untouched.
    let snapshot = parent
        .read_range(Path::new(&format!("{ROOT}/snapshots/{SNAP}")), 0, 100)
        .await
        .expect("still there");
    assert_eq!(snapshot, b"snapshot");
}
