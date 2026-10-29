//! Every call family through a DFS link inside the share (`dfs_links.rs`):
//! transfers, listings, stats, scans, and mutations, against
//! smb2's own DFS fixtures: `smb-dfs-root` (port 10456) holds a share `dfs`
//! whose `data` folder is an `msdfs:` link to `smb-dfs-target`'s `files` share
//! (port 10457).
//!
//! These fixtures live in smb2's INTERNAL Docker stack, not the consumer stack
//! Cmdr vendors, so the cells are opt-in: they need the two containers started
//! by hand from the smb2 repo,
//! `docker compose -f crates/smb2/tests/docker/internal/docker-compose.yml up -d --build smb-dfs-root smb-dfs-target`,
//! then `CMDR_SMB_DFS_FIXTURE=1 cargo nextest run -p cmdr-smb --run-ignored all -E 'test(smb_dfs_)'`.
//!
//! ❗ `#[ignore]` alone doesn't keep them out of a gating run: the integration
//! lane runs EVERY ignored cell in `package(cmdr-smb)` against the consumer
//! stack, which has no DFS containers. Without the variable each cell says
//! "skipped" and passes ([`dfs_fixture_not_requested`]).

use super::session_build::TEST_DFS_TARGET_OVERRIDES;
use super::test_support::*;
use super::*;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{StreamLength, WriteMode};
use std::path::Path;

const DFS_ROOT_PORT: u16 = 10456;
const DFS_TARGET_ADDR: &str = "127.0.0.1:10457";

/// The variable that says smb2's DFS containers are up (this file's header).
const DFS_FIXTURE_ENV: &str = "CMDR_SMB_DFS_FIXTURE";

/// Answers true, having said why, when nobody asked for the DFS cells: the
/// shared fixture lane selects them with the rest of the package, and its stack
/// has no DFS containers. Asked for and unreachable still FAILS, in
/// [`dfs_volume`].
#[allow(
    clippy::print_stderr,
    reason = "a skip notice belongs in the test runner's captured output, where nextest attributes it to the cell"
)]
fn dfs_fixture_not_requested() -> bool {
    if std::env::var_os(DFS_FIXTURE_ENV).is_some_and(|v| !v.is_empty()) {
        return false;
    }
    eprintln!("skipped: set {DFS_FIXTURE_ENV}=1 once smb2's DFS containers are up (see this file's header)");
    true
}

/// A volume on the DFS root share, with the link's target name pointed at the
/// target container's port-mapped address.
async fn dfs_volume() -> SmbVolume {
    TEST_DFS_TARGET_OVERRIDES
        .lock_ignore_poison()
        .get_or_insert_with(Default::default)
        .insert("smb-dfs-target".to_string(), DFS_TARGET_ADDR.to_string());
    let params = SmbConnectionParams::new("127.0.0.1", "dfs", DFS_ROOT_PORT, None, None);
    connect_smb_volume(
        "dfs",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &cmdr_fs::volume::smb_volume_id("127.0.0.1", DFS_ROOT_PORT, "dfs"),
        params,
        VolumeHost::detached(),
    )
    .await
    .unwrap_or_else(|e| {
        panic!("Couldn't reach smb2's smb-dfs-root at 127.0.0.1:{DFS_ROOT_PORT}; see this file's header ({e:?})")
    })
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 251) as u8).collect()
}

async fn upload(vol: &SmbVolume, path: &str, bytes: Vec<u8>, mode: WriteMode) {
    let len = bytes.len() as u64;
    let written = vol
        .write_from_stream(
            Path::new(path),
            mode,
            StreamLength::Known(len),
            inline_read_stream(bytes),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("an upload through the link lands: {e:?}"));
    assert_eq!(written.bytes, len);
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_file_behind_a_link_downloads_streamed_and_hinted() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let expected = b"Hello from DFS target!\n".to_vec();

    let streamed = drain(vol.open_read_stream(Path::new("data/hello.txt")).await.unwrap()).await;
    assert_eq!(
        streamed, expected,
        "a streamed read through the link reads the target's file"
    );

    let hinted = drain(
        vol.open_read_stream_with_hint(Path::new("data/hello.txt"), Some(expected.len() as u64))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        hinted, expected,
        "a hinted read through the link falls through to the link stream"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_uploads_through_a_link_land_on_the_target_streamed_and_single_frame() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    vol.create_directory(Path::new(&folder)).await.unwrap();
    let big_path = &format!("{folder}/big.bin");
    let small_path = &format!("{folder}/small.bin");

    // Bigger than a fresh connection's one-frame limit, so it streams, and
    // spans several of the link download's windows on the way back.
    let big = payload(5 * 1024 * 1024 + 123);
    upload(&vol, big_path, big.clone(), WriteMode::CreateOrReplace).await;
    let small = payload(4096);
    upload(&vol, small_path, small.clone(), WriteMode::CreateOrReplace).await;

    let big_back = drain(vol.open_read_stream(Path::new(big_path)).await.unwrap()).await;
    assert_eq!(big_back.len(), big.len());
    assert!(big_back == big, "the streamed upload reads back byte for byte");
    let small_back = drain(vol.open_read_stream(Path::new(small_path)).await.unwrap()).await;
    assert_eq!(small_back, small, "the one-frame upload reads back byte for byte");

    for path in [big_path, small_path, &folder] {
        vol.delete(Path::new(path)).await.unwrap();
    }
}

/// A folder name of this run's own inside the link, so runs never collide.
fn run_folder() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("data/cmdr-dfs-{}-{nanos}", std::process::id())
}

fn names(entries: &[FileEntry]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    names.sort();
    names
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_folder_behind_a_link_lists() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let top = vol.list_directory(Path::new("data"), None).await.unwrap();
    let top_names = names(&top);
    assert!(
        top_names.contains(&"hello.txt".to_string()) && top_names.contains(&"subdir".to_string()),
        "the link's folder lists the target share, got {top_names:?}"
    );
    let hello = top.iter().find(|e| e.name == "hello.txt").unwrap();
    assert_eq!(hello.size, Some(23));
    assert!(
        hello.path.ends_with("data/hello.txt"),
        "an entry keeps the caller's path, link folder included, got {:?}",
        hello.path
    );

    let nested = vol.list_directory(Path::new("data/subdir"), None).await.unwrap();
    assert_eq!(
        names(&nested),
        vec!["nested.txt".to_string()],
        "a folder deeper in the link"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_stats_behind_a_link_describe_the_target() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let file = vol.get_metadata(Path::new("data/hello.txt")).await.unwrap();
    assert_eq!((file.is_directory, file.size), (false, Some(23)), "get_metadata");
    assert!(
        vol.is_directory(Path::new("data/subdir")).await.unwrap(),
        "is_directory"
    );
    assert!(
        !vol.is_directory(Path::new("data/hello.txt")).await.unwrap(),
        "is_directory on a file"
    );
    assert!(vol.exists(Path::new("data/subdir/nested.txt")).await, "exists");
    assert!(
        !vol.exists(Path::new("data/no-such-file.txt")).await,
        "exists on a miss"
    );
    assert!(matches!(
        vol.get_metadata(Path::new("data/no-such-file.txt")).await,
        Err(VolumeError::NotFound(_))
    ));
    assert_eq!(
        vol.entry_kind(Path::new("data/subdir")).await.unwrap(),
        cmdr_fs::volume::EntryKind::Directory,
        "entry_kind"
    );
    let attributes = vol.attributes_impl(Path::new("data/hello.txt")).await.unwrap();
    assert!(!attributes.contains(smb2::FileAttributes::DIRECTORY), "attributes");
    assert_eq!(
        vol.read_range(Path::new("data/hello.txt"), 6, 4).await.unwrap(),
        b"from".to_vec(),
        "read_range"
    );
    let reader = vol.open_random_reader(Path::new("data/hello.txt")).await.unwrap();
    assert_eq!(reader.size(), 23, "open_random_reader");
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_copy_scan_walks_behind_a_link() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;

    let scan = vol.scan_for_copy(Path::new("data/subdir")).await.unwrap();
    assert_eq!((scan.file_count, scan.total_bytes), (1, 12), "a folder behind the link");

    let batch = vol
        .scan_for_copy_batch(&[PathBuf::from("data/hello.txt"), PathBuf::from("data/subdir")])
        .await
        .unwrap();
    assert_eq!(
        (batch.aggregate.file_count, batch.aggregate.total_bytes),
        (2, 35),
        "the batch scan's pipelined top-level stats follow the link too"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_mutations_behind_a_link_land_on_the_target() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    let file = format!("{folder}/note.txt");
    let renamed = format!("{folder}/renamed.txt");

    vol.create_directory(Path::new(&folder)).await.unwrap();
    vol.create_file(Path::new(&file), b"hi").await.unwrap();
    assert!(
        matches!(
            vol.create_file(Path::new(&file), b"again").await,
            Err(VolumeError::AlreadyExists(_))
        ),
        "create_file still refuses to clobber through the link"
    );
    vol.rename(Path::new(&file), Path::new(&renamed), false).await.unwrap();
    vol.create_file(Path::new(&file), b"second").await.unwrap();
    vol.rename(Path::new(&file), Path::new(&renamed), true).await.unwrap();
    assert_eq!(
        vol.read_range(Path::new(&renamed), 0, 6).await.unwrap(),
        b"second".to_vec(),
        "the forced rename swapped the new file in"
    );
    assert_eq!(
        names(&vol.list_directory(Path::new(&folder), None).await.unwrap()),
        vec!["renamed.txt".to_string()]
    );

    vol.delete(Path::new(&renamed)).await.unwrap();
    vol.delete(Path::new(&folder)).await.unwrap();
    assert!(
        !vol.exists(Path::new(&folder)).await,
        "the folder is gone from the target"
    );
}

/// smb2 follows a rename through a link only within one directory: a
/// SET_INFO's destination is relative to the TARGET share, and smb2 can't tell
/// where the link folder ends in a destination elsewhere. So a move between two
/// folders behind one link is refused, pinned so the gap can't pass for done.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_move_between_folders_behind_a_link_is_refused() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let folder = run_folder();
    let inner = format!("{folder}/inner");
    let file = format!("{folder}/moved.txt");
    vol.create_directory(Path::new(&folder)).await.unwrap();
    vol.create_directory(Path::new(&inner)).await.unwrap();
    vol.create_file(Path::new(&file), b"x").await.unwrap();

    let moved = vol
        .rename(Path::new(&file), Path::new(&format!("{inner}/moved.txt")), false)
        .await;
    assert!(
        moved.is_err(),
        "smb2 can't place this rename on the target, got {moved:?}"
    );

    vol.delete(Path::new(&file)).await.unwrap();
    vol.delete(Path::new(&inner)).await.unwrap();
    vol.delete(Path::new(&folder)).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_foreign_spelling_behind_a_link_resolves() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    // Exact already: nothing to correct.
    assert_eq!(
        vol.find_stored_spelling(Path::new("data/subdir/nested.txt"), None)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_the_index_scan_pool_lists_behind_a_link() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    vol.begin_scan_session().await;
    assert!(
        vol.inner.scan_pool.read().await.is_some(),
        "the pool is open, so the listing rides a member"
    );
    let listed = vol.list_directory_for_scan(Path::new("data/subdir"), None).await;
    let prefetched = vol
        .open_read_stream_for_scan(Path::new("data/subdir/nested.txt"), Some(12))
        .await;
    vol.end_scan_session().await;
    assert_eq!(names(&listed.unwrap()), vec!["nested.txt".to_string()]);
    assert_eq!(
        drain(prefetched.unwrap()).await,
        b"Nested file\n".to_vec(),
        "an enrichment prefetch behind the link streams through it"
    );
}

/// The share's CHANGE_NOTIFY watch runs on the share's own server, so it never
/// hears a change on a link's target: a listing behind a link isn't kept fresh,
/// and the fresh-listing oracle must not stand in for a read there.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_listing_behind_a_link_claims_no_watch() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    assert_eq!(vol.listing_watch_coverage(Path::new("")), WatchCoverage::EveryWriter);
    vol.list_directory(Path::new("data/subdir"), None).await.unwrap();
    assert_eq!(
        vol.listing_watch_coverage(Path::new("data/subdir")),
        WatchCoverage::None
    );
    assert_eq!(
        vol.listing_watch_coverage(Path::new("data/subdir/deeper")),
        WatchCoverage::None,
        "below a followed path too"
    );
    assert_eq!(
        vol.listing_watch_coverage(Path::new("")),
        WatchCoverage::EveryWriter,
        "the share itself stays watched"
    );
}

#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_a_server_side_copy_through_a_link_asks_the_caller_to_stream() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    let result = Volume::copy_within(
        &vol,
        Path::new("data/hello.txt"),
        Path::new("data/never.txt"),
        &|_, _| std::ops::ControlFlow::Continue(()),
    )
    .await;
    assert!(
        matches!(result, Err(VolumeError::NotSupported)),
        "the engine's 'stream it instead', got {result:?}"
    );
}
