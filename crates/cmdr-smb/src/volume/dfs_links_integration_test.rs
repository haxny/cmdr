//! Transfers through a DFS link inside the share (`dfs_links.rs`), against
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

use super::session::TEST_DFS_TARGET_OVERRIDES;
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
    // Fixed names, replaced on every run: the delete that would tidy them is a
    // main-session mutation, which doesn't follow a link yet (the cell below).
    let big_path = "data/cmdr-transfer-session-big.bin";
    let small_path = "data/cmdr-transfer-session-small.bin";

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
}

/// What doesn't follow a link yet, pinned so the gap can't be mistaken for
/// done: the main session's listings, `stat`s, and mutations all drive the
/// `Tree` API, so a path behind a link answers `STATUS_PATH_NOT_COVERED`. A
/// copy INTO or OUT OF a link therefore still fails at its scan and at its
/// landing rename, though every byte-moving call above follows the link. When
/// those learn to follow it too, this cell flips and goes.
#[tokio::test]
#[ignore = "Requires smb2's smb-dfs-root and smb-dfs-target containers and CMDR_SMB_DFS_FIXTURE=1 (see this file's header)"]
async fn smb_dfs_the_main_sessions_calls_dont_follow_a_link_yet() {
    if dfs_fixture_not_requested() {
        return;
    }
    let vol = dfs_volume().await;
    assert!(vol.list_directory(Path::new("data"), None).await.is_err(), "a listing");
    assert!(vol.get_metadata(Path::new("data/hello.txt")).await.is_err(), "a stat");
    assert!(vol.scan_for_copy(Path::new("data")).await.is_err(), "a copy scan");
    assert!(
        vol.create_directory(Path::new("data/never")).await.is_err(),
        "a mutation"
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
