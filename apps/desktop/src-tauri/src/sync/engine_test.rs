//! The adapter on real temp folders, through a test engine of its own.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, unique_test_id, wait_until_async};
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_local::LocalPosixVolume;
use cmdr_operations::OperationEventSink;
use cmdr_operations::testing::{CollectorEventSink, TestOperations, TrashCleanup};
use cmdr_sync::{
    BatchOutcome, BoundSource, EngineBatch, EntryKind, ItemOutcome, MaskFilter, NameRule, Place, ScanOptions, Side,
    SyncControl, SyncEngine,
};

use super::engine::{EngineAdapter, EngineConfig, EngineSide};

pub(super) fn scan_options() -> ScanOptions {
    ScanOptions {
        subfolders: true,
        include_hidden: true,
        empty_folders: false,
        filter: MaskFilter::parse("", &[]).expect("no masks parse"),
        name_rule: NameRule::FoldCase,
        only: None,
    }
}

/// A test engine of its own, with the local disk registered as `root`. Leaked:
/// the adapter holds the engine for `'static`, as the app's.
pub(super) fn test_engine() -> &'static TestOperations {
    let engine: &'static TestOperations = Box::leak(Box::new(TestOperations::new()));
    let root: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Test root",
        "/",
        crate::volume_host::host(),
    ));
    engine.volumes().register("root", root);
    engine
}

pub(super) fn local_side(engine: &TestOperations, tag: &str, root: &Path) -> EngineSide {
    let volume_id = unique_test_id(tag);
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(tag, root, crate::volume_host::host()));
    engine.volumes().register(&volume_id, Arc::clone(&volume));
    EngineSide {
        volume_id,
        volume,
        root: root.to_path_buf(),
    }
}

struct Fixture {
    _dir: TestDir,
    left: PathBuf,
    right: PathBuf,
    adapter: EngineAdapter,
    control: Arc<SyncControl>,
    sink: Arc<CollectorEventSink>,
}

fn fixture(label: &str) -> Fixture {
    let dir = TestDir::new(label);
    let (left, right) = (dir.join("L"), dir.join("R"));
    std::fs::create_dir_all(&left).expect("left root");
    std::fs::create_dir_all(&right).expect("right root");
    let engine = test_engine();
    let control = SyncControl::new();
    let sink = Arc::new(CollectorEventSink::new());
    let shared = Arc::clone(&sink);
    let adapter = EngineAdapter::new(EngineConfig {
        operations: engine,
        left: local_side(engine, "sync-left", &left),
        right: local_side(engine, "sync-right", &right),
        local: local_side(engine, "sync-local", Path::new("/")),
        scan: scan_options(),
        control: Arc::clone(&control),
        sinks: Arc::new(move || Arc::clone(&shared) as Arc<dyn OperationEventSink>),
    });
    Fixture {
        _dir: dir,
        left,
        right,
        adapter,
        control,
        sink,
    }
}

impl Fixture {
    async fn bound(&self, side: Side, path: &Path) -> BoundSource {
        let observed = self
            .adapter
            .observe(side, Place::Target, path, false)
            .await
            .expect("the source is there");
        BoundSource {
            path: path.to_path_buf(),
            expected: observed.facts,
        }
    }

    async fn copy_right(&self, source: BoundSource) -> BatchOutcome {
        let batch = EngineBatch::Copy {
            from: Side::Left,
            sources: vec![source],
            dest_dir: self.right.clone(),
        };
        self.adapter.run(batch).await
    }
}

fn only(outcome: &BatchOutcome) -> ItemOutcome {
    assert_eq!(outcome.items.len(), 1, "one source, one answer: {outcome:?}");
    outcome.items[0].1
}

fn touch_later(path: &Path, secs: u64) {
    let file = std::fs::File::options().write(true).open(path).expect("open");
    let later = SystemTime::now() + Duration::from_secs(secs);
    file.set_modified(later).expect("set mtime");
}

#[tokio::test]
async fn engine_copies_a_bound_file_and_reports_done() {
    let f = fixture("sync_engine_copy");
    let source = f.left.join("a.txt");
    std::fs::write(&source, b"hello").expect("write");
    let outcome = f.copy_right(f.bound(Side::Left, &source).await).await;
    assert_eq!(only(&outcome), ItemOutcome::Done);
    assert!(!outcome.stopped);
    assert_eq!(std::fs::read(f.right.join("a.txt")).expect("landed"), b"hello");
    assert!(!f.adapter.operation_ids().is_empty(), "the copy is an engine operation");
}

#[tokio::test]
async fn engine_skips_source_changed_after_compare() {
    let f = fixture("sync_engine_changed");
    let source = f.left.join("a.txt");
    std::fs::write(&source, b"hello").expect("write");
    let bound = f.bound(Side::Left, &source).await;
    // Same size, a new time: only the time (in seconds) tells.
    touch_later(&source, 30);
    let outcome = f.copy_right(bound).await;
    assert_eq!(only(&outcome), ItemOutcome::Skipped);
    assert!(!f.right.join("a.txt").exists());
}

#[tokio::test]
async fn copy_never_overwrites_a_file_that_appeared_after_compare() {
    let f = fixture("sync_engine_no_overwrite");
    let source = f.left.join("a.txt");
    std::fs::write(&source, b"new content").expect("write");
    let bound = f.bound(Side::Left, &source).await;
    std::fs::write(f.right.join("a.txt"), b"theirs").expect("appeared");
    let outcome = f.copy_right(bound).await;
    assert_ne!(only(&outcome), ItemOutcome::Done);
    assert_eq!(std::fs::read(f.right.join("a.txt")).expect("kept"), b"theirs");
}

#[tokio::test]
async fn claim_folder_is_exclusive() {
    let f = fixture("sync_engine_claim");
    let dir = f.right.join(".cmdr-sync/versions/2026-11-01 093000");
    let claim = || EngineBatch::ClaimFolder {
        place: Place::Target,
        side: Side::Right,
        dir: dir.clone(),
    };
    assert_eq!(only(&f.adapter.run(claim()).await), ItemOutcome::Done);
    assert!(dir.is_dir());
    assert_eq!(only(&f.adapter.run(claim()).await), ItemOutcome::Skipped);
}

#[tokio::test]
async fn remove_empty_folders_refuses_hidden_files_and_symlinks() {
    let f = fixture("sync_engine_rmdir");
    let empty = f.right.join("empty");
    let hidden = f.right.join("hidden");
    let target = f.right.join("target");
    let link = f.right.join("link");
    for dir in [&empty, &hidden, &target] {
        std::fs::create_dir(dir).expect("mkdir");
    }
    std::fs::write(hidden.join(".DS_Store"), b"x").expect("hidden file");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let batch = EngineBatch::RemoveEmptyFolders {
        place: Place::Target,
        side: Side::Right,
        dirs: vec![empty.clone(), hidden.clone(), link.clone()],
    };
    let outcome = f.adapter.run(batch).await;
    assert_eq!(
        outcome.items,
        vec![
            (empty.clone(), ItemOutcome::Done),
            (hidden.clone(), ItemOutcome::Failed),
            (link.clone(), ItemOutcome::Failed),
        ]
    );
    assert!(!empty.exists());
    assert!(hidden.join(".DS_Store").exists());
    assert!(std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()));
    assert!(target.is_dir());
}

#[tokio::test]
async fn observe_reads_a_symlink_as_a_symlink() {
    let f = fixture("sync_engine_symlink");
    let folder = f.left.join("folder");
    std::fs::create_dir(&folder).expect("mkdir");
    std::fs::write(folder.join("x.txt"), b"x").expect("write");
    let link = f.left.join("link");
    std::os::unix::fs::symlink(&folder, &link).expect("symlink");
    let seen = f.adapter.observe(Side::Left, Place::Target, &link, true).await;
    let seen = seen.expect("the link is there");
    assert_eq!(seen.facts.kind, EntryKind::Symlink);
    assert_eq!(seen.subtree, None, "a link is never walked");
    let folder_seen = f.adapter.observe(Side::Left, Place::Target, &folder, true).await;
    let subtree = folder_seen.and_then(|seen| seen.subtree).expect("a folder's subtree");
    assert_eq!(subtree.files, 1);
}

#[tokio::test]
async fn cancel_stops_a_running_move_but_never_a_move_back() {
    let f = fixture("sync_engine_cancel");
    let locked = f.left.join("locked.txt");
    std::fs::write(&locked, b"locked").expect("write");
    let status = std::process::Command::new("chflags").arg("uchg").arg(&locked).status();
    assert!(status.is_ok_and(|s| s.success()), "chflags uchg");
    let aside = f.left.join(".cmdr-sync/versions/aside");
    std::fs::create_dir_all(&aside).expect("aside");

    // A move to another volume (here a custom versions folder on the local
    // disk) parks on "these items are Locked"; the run's cancel ends it.
    let bound = f.bound(Side::Left, &locked).await;
    let control = Arc::clone(&f.control);
    let sink = Arc::clone(&f.sink);
    let canceller = tokio::spawn(async move {
        wait_until_async(Duration::from_secs(4), "the move asks about the Locked file", || {
            !sink.locked_sources.lock_ignore_poison().is_empty()
        })
        .await;
        control.cancel();
    });
    let batch = EngineBatch::Move {
        side: Side::Left,
        sources: vec![bound],
        place: Place::LocalDisk,
        dest_dir: aside.clone(),
    };
    let outcome = tokio::time::timeout(Duration::from_secs(20), f.adapter.run(batch))
        .await
        .expect("the cancel reaches the parked move");
    canceller.await.expect("canceller");
    assert!(outcome.stopped);
    assert!(locked.exists());
    let _ = std::process::Command::new("chflags")
        .arg("nouchg")
        .arg(&locked)
        .status();

    // Cancelled now: a new copy never starts, a move back still runs.
    let source = f.left.join("b.txt");
    std::fs::write(&source, b"b").expect("write");
    let copy = f.copy_right(f.bound(Side::Left, &source).await).await;
    assert!(copy.stopped);
    assert!(!f.right.join("b.txt").exists());
    let staged = aside.join("c.txt");
    std::fs::write(&staged, b"c").expect("staged");
    let bound = f.bound(Side::Left, &staged).await;
    let back = EngineBatch::MoveBack {
        side: Side::Left,
        place: Place::Target,
        sources: vec![bound],
        dest_dir: f.left.clone(),
    };
    let back = f.adapter.run_to_end(back).await;
    assert_eq!(only(&back), ItemOutcome::Done);
    assert!(f.left.join("c.txt").exists());
    assert!(!staged.exists());
}

#[tokio::test]
async fn trash_goes_through_the_engine_and_lands_in_the_trash() {
    let f = fixture("sync_engine_trash");
    let old = f.right.join("old.txt");
    std::fs::write(&old, b"old").expect("write");
    let _cleanup = TrashCleanup::of(&[&old]);
    let bound = f.bound(Side::Right, &old).await;
    let outcome = f
        .adapter
        .run(EngineBatch::Trash {
            side: Side::Right,
            sources: vec![bound],
        })
        .await;
    assert_eq!(only(&outcome), ItemOutcome::Done);
    assert!(!old.exists());
}

#[tokio::test]
async fn copies_to_a_volume_without_a_local_path_bind_remote_fingerprints() {
    let dir = TestDir::new("sync_engine_remote");
    let left = dir.join("L");
    std::fs::create_dir_all(&left).expect("left");
    let engine = test_engine();
    let remote_id = unique_test_id("sync-remote");
    let remote = InMemoryVolume::new("Remote").with_lane_key(remote_id.clone());
    remote.create_directory(Path::new("/R")).await.expect("remote root");
    let remote: Arc<dyn Volume> = Arc::new(remote);
    engine.volumes().register(&remote_id, Arc::clone(&remote));
    let sink = Arc::new(CollectorEventSink::new());
    let adapter = EngineAdapter::new(EngineConfig {
        operations: engine,
        left: local_side(engine, "sync-left", &left),
        right: EngineSide {
            volume_id: remote_id,
            volume: Arc::clone(&remote),
            root: PathBuf::from("/R"),
        },
        local: local_side(engine, "sync-local", Path::new("/")),
        scan: scan_options(),
        control: SyncControl::new(),
        sinks: Arc::new(move || Arc::clone(&sink) as Arc<dyn OperationEventSink>),
    });
    let source = left.join("a.txt");
    std::fs::write(&source, b"remote").expect("write");
    let seen = adapter.observe(Side::Left, Place::Target, &source, false).await;
    let bound = BoundSource {
        path: source.clone(),
        expected: seen.expect("there").facts,
    };
    let outcome = adapter
        .run(EngineBatch::Copy {
            from: Side::Left,
            sources: vec![bound],
            dest_dir: PathBuf::from("/R"),
        })
        .await;
    assert_eq!(only(&outcome), ItemOutcome::Done);
    let landed = adapter
        .observe(Side::Right, Place::Target, Path::new("/R/a.txt"), false)
        .await
        .expect("landed on the remote side");
    assert_eq!(landed.facts.size, Some(6));
    // Back the other way, bound with the remote fingerprint.
    std::fs::remove_file(&source).expect("clear the left");
    let back = adapter
        .run(EngineBatch::Copy {
            from: Side::Right,
            sources: vec![BoundSource {
                path: PathBuf::from("/R/a.txt"),
                expected: landed.facts,
            }],
            dest_dir: left.clone(),
        })
        .await;
    assert_eq!(only(&back), ItemOutcome::Done);
    assert_eq!(std::fs::read(&source).expect("back"), b"remote");
}
