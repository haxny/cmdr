//! What letting go of a volume does to the repos on it (`GitPortal::release_volume`), against
//! repos in a temp dir standing in for the volume. The real unmount it exists for is
//! `fault_guard_real_image::an_open_repo_lets_its_volume_unmount_once_released`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cmdr_fs::testing::wait_until;
use cmdr_fs::volume::git_error::FriendlyGitErrorKind;
use cmdr_fs::volume::host::VolumeHost;

use crate::fault_guard::GuardedRead;
use crate::test_fixtures::{Fixture, cleanup, temp_dir};
use crate::{GitPortal, no_git_state_sink};

/// A repo with one commit: its directory and its canonical root, the spelling a mount root has.
fn a_repo(name: &str) -> (PathBuf, PathBuf) {
    let dir = temp_dir("releasing", name);
    Fixture::init(dir.clone()).commit_file("README.md", b"hello\n", "initial");
    let root = dir.canonicalize().expect("the repo dir exists");
    (dir, root)
}

fn portal() -> GitPortal {
    GitPortal::with_scripted_watcher(VolumeHost::detached(), no_git_state_sink())
}

#[test]
fn a_released_volume_lets_go_of_its_repos_and_opens_none_until_the_hold_ends() {
    let (dir, root) = a_repo("drops_and_refuses");
    let portal = portal();
    drop(portal.discover(&dir).expect("open the repo"));

    assert!(
        portal.release_volume(&root, Instant::now() + Duration::from_secs(5)),
        "nothing else held the repo, so it let go at once"
    );

    assert_eq!(portal.repos().len(), 0);
    let refused = portal.discover(&dir).expect_err("its volume is leaving");
    assert_eq!(refused.kind, FriendlyGitErrorKind::VolumeGone);
    cleanup(&dir);
}

#[test]
fn a_read_still_holding_the_repo_keeps_the_release_waiting_until_it_lets_go() {
    let (dir, root) = a_repo("waits_for_the_read");
    let portal = portal();
    let (in_flight, _) = portal.discover(&dir).expect("open the repo");

    assert!(
        !portal.release_volume(&root, Instant::now() + Duration::from_millis(50)),
        "a handle still in use keeps the pack mapped, so the volume can't unmount yet"
    );
    drop(in_flight);
    assert!(portal.release_volume(&root, Instant::now() + Duration::from_millis(50)));
    cleanup(&dir);
}

#[test]
fn a_walk_on_a_released_volume_is_told_to_stop_and_answers_volume_gone() {
    let (dir, root) = a_repo("stops_the_walk");
    let read = GuardedRead::begin(&root);
    let stop = read.stop_flag();

    let released = portal().release_volume(&root, Instant::now());

    assert!(!released, "the read was still in flight at the deadline");
    assert!(stop.load(std::sync::atomic::Ordering::SeqCst), "gix polls this flag");
    let refused = read.refusal().expect("a stopped read is never trusted");
    assert_eq!(refused.kind, FriendlyGitErrorKind::VolumeGone);
    cleanup(&dir);
}

#[test]
fn a_repo_on_another_volume_is_left_alone() {
    let (dir, _root) = a_repo("left_alone");
    let (other_dir, other_root) = a_repo("left_alone_other");
    let portal = portal();
    drop(portal.discover(&dir).expect("open the repo"));
    drop(portal.discover(&other_dir).expect("open the other repo"));

    assert!(portal.release_volume(&other_root, Instant::now()));

    assert!(portal.discover(&dir).is_ok());
    assert_eq!(portal.repos().len(), 1, "only the leaving volume's repo was dropped");
    cleanup(&dir);
    cleanup(&other_dir);
}

#[test]
fn the_refusal_to_reopen_ends_with_its_hold() {
    let (dir, root) = a_repo("hold_ends");
    let portal = portal();
    let _stores = portal
        .repos()
        .let_go_under(&root, Instant::now() + Duration::from_millis(30));
    assert!(portal.discover(&dir).is_err());

    // An unmount that was refused hands git back once the hold is over.
    wait_until(Duration::from_secs(5), "the repo to open again", || {
        portal.discover(&dir).is_ok()
    });
    cleanup(&dir);
}
