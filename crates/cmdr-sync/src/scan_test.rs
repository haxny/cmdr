use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};

use crate::*;

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, VolumeError>> + Send + 'a>>;

/// Delegates to an `InMemoryVolume` but refuses to list one path.
struct FailingListVolume {
    inner: InMemoryVolume,
    fail_path: PathBuf,
    error: fn() -> VolumeError,
}

impl Volume for FailingListVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Vec<FileEntry>> {
        if path == self.fail_path {
            let error = (self.error)();
            return Box::pin(async move { Err(error) });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, FileEntry> {
        self.inner.get_metadata(path)
    }
}

fn denied() -> VolumeError {
    VolumeError::PermissionDenied {
        path: "test".into(),
        raw_os_error: None,
    }
}

async fn tree(files: &[(&str, usize)], dirs: &[&str]) -> InMemoryVolume {
    let volume = InMemoryVolume::new("t").with_sibling_duplicates_allowed();
    for dir in dirs {
        volume.create_directory(Path::new(dir)).await.unwrap();
    }
    for (path, size) in files {
        volume.create_file(Path::new(path), &vec![7u8; *size]).await.unwrap();
        volume.set_modified_at(Path::new(path), Some(1_700_000_000));
    }
    volume
}

fn options() -> ScanOptions {
    ScanOptions {
        subfolders: true,
        include_hidden: true,
        empty_folders: true,
        filter: MaskFilter::parse("", &[]).unwrap(),
        name_rule: NameRule::FoldCase,
        only: None,
    }
}

async fn run(volume: &dyn Volume, options: &ScanOptions) -> Result<Snapshot, ScanError> {
    scan(
        Side::Left,
        volume,
        Path::new("/"),
        options,
        &SyncControl::new(),
        &|_| {},
    )
    .await
}

fn key(parts: &[&str]) -> EntryKey {
    EntryKey::Name(parts.iter().map(|p| p.to_string()).collect())
}

#[tokio::test]
async fn scan_lists_the_tree_with_subtree_facts() {
    let volume = tree(&[("/a.txt", 3), ("/d/b.txt", 5), ("/d/e/c.txt", 10)], &["/d", "/d/e"]).await;
    let snap = run(&volume, &options()).await.unwrap();
    assert_eq!(snap.entries.len(), 5);
    assert_eq!(snap.file_count(), 3);
    let d = &snap.entries[&key(&["d"])];
    assert_eq!(d.rel_path, "d");
    let sub = d.subtree.unwrap();
    assert_eq!((sub.files, sub.bytes, sub.held_back), (2, 15, 0));
    assert_eq!(sub.newest_mtime, Some(1_700_000_000));
    assert_eq!(snap.entries[&key(&["d", "e", "c.txt"])].rel_path, "d/e/c.txt");
    assert!(snap.entries[&key(&["a.txt"])].subtree.is_none());
    assert_eq!(snap.entries[&key(&["d", "e"])].subtree.unwrap().files, 1);
}

#[tokio::test]
async fn scan_missing_root_is_unreachable() {
    // `InMemoryVolume` lists a missing folder as empty, so the double refuses it.
    let volume = FailingListVolume {
        inner: InMemoryVolume::new("t"),
        fail_path: PathBuf::from("/gone"),
        error: || VolumeError::NotFound("/gone".into()),
    };
    let err = scan(
        Side::Right,
        &volume,
        Path::new("/gone"),
        &options(),
        &SyncControl::new(),
        &|_| {},
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ScanError::RootUnreachable { side: Side::Right, .. }));
}

#[tokio::test]
async fn scan_empty_root_is_an_empty_snapshot() {
    let snap = run(&InMemoryVolume::new("t"), &options()).await.unwrap();
    assert!(snap.is_empty());
    assert_eq!(snap.file_count(), 0);
}

#[tokio::test]
async fn scan_prunes_excluded_folders_and_counts_them_held_back() {
    let volume = tree(
        &[("/d/keep.txt", 1), ("/d/raw/x.jpg", 1), ("/d/skip.tmp", 1)],
        &["/d", "/d/raw"],
    )
    .await;
    let mut opts = options();
    opts.filter = MaskFilter::parse("| raw/ *.tmp", &[]).unwrap();
    let snap = run(&volume, &opts).await.unwrap();
    assert!(!snap.entries.contains_key(&key(&["d", "raw"])));
    assert!(!snap.entries.contains_key(&key(&["d", "raw", "x.jpg"])));
    let sub = snap.entries[&key(&["d"])].subtree.unwrap();
    assert_eq!(sub.held_back, 2);
    assert_eq!(sub.files, 1);
}

#[tokio::test]
async fn excluded_folders_are_never_listed() {
    let volume = FailingListVolume {
        inner: tree(&[("/raw/x.jpg", 1), ("/a.txt", 1)], &["/raw"]).await,
        fail_path: PathBuf::from("/raw"),
        error: denied,
    };
    let mut opts = options();
    opts.filter = MaskFilter::parse("| raw/", &[]).unwrap();
    let snap = run(&volume, &opts).await.unwrap();
    assert!(snap.unreadable.is_empty());
    assert_eq!(snap.file_count(), 1);
}

#[tokio::test]
async fn excluded_items_never_reach_the_snapshot() {
    let volume = tree(
        &[
            ("/.DS_Store", 1),
            ("/ok.txt", 1),
            ("/.hidden", 1),
            ("/.cmdr-sync/log", 1),
        ],
        &["/.cmdr-sync"],
    )
    .await;
    let mut opts = options();
    opts.include_hidden = false;
    opts.filter = MaskFilter::parse("", &DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
    let snap = run(&volume, &opts).await.unwrap();
    let names: Vec<_> = snap.entries.values().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(names, ["ok.txt"]);
    assert!(snap.clashes.is_empty());
}

#[tokio::test]
async fn hidden_entries_count_as_held_back_when_off() {
    let volume = tree(&[("/d/.hid", 1), ("/d/seen", 1), ("/d/.DS_Store", 1)], &["/d"]).await;
    let mut opts = options();
    opts.include_hidden = false;
    opts.filter = MaskFilter::parse("", &[".DS_Store".to_string()]).unwrap();
    let snap = run(&volume, &opts).await.unwrap();
    let sub = snap.entries[&key(&["d"])].subtree.unwrap();
    // The hidden file is held back; the junk file is neither an entry nor held back.
    assert_eq!((sub.files, sub.held_back), (1, 1));
}

#[tokio::test]
async fn unreadable_subfolder_is_recorded_and_the_walk_goes_on() {
    let volume = FailingListVolume {
        inner: tree(&[("/bad/x", 1), ("/good/y", 1)], &["/bad", "/good"]).await,
        fail_path: PathBuf::from("/bad"),
        error: denied,
    };
    let snap = run(&volume, &options()).await.unwrap();
    assert_eq!(
        snap.unreadable.get(&key(&["bad"])),
        Some(&UnreadableReason::PermissionDenied)
    );
    assert!(snap.entries.contains_key(&key(&["bad"])));
    assert!(snap.entries.contains_key(&key(&["good", "y"])));
}

#[tokio::test]
async fn nfd_left_and_nfc_right_pair_with_their_own_spellings() {
    let nfd = "e\u{301}.txt";
    let nfc = "\u{e9}.txt";
    let left = tree(&[(&format!("/{nfd}"), 1)], &[]).await;
    let right = tree(&[(&format!("/{nfc}"), 1)], &[]).await;
    let l = run(&left, &options()).await.unwrap();
    let r = run(&right, &options()).await.unwrap();
    let (lk, le) = l.entries.iter().next().unwrap();
    let (rk, re) = r.entries.iter().next().unwrap();
    assert_eq!(lk, rk);
    assert_eq!(le.rel_path, nfd);
    assert_eq!(re.rel_path, nfc);
}

#[tokio::test]
async fn case_twins_on_one_side_become_a_clash() {
    let volume = tree(&[("/A.txt", 1), ("/a.txt", 2), ("/b.txt", 1)], &[]).await;
    let snap = run(&volume, &options()).await.unwrap();
    assert_eq!(snap.clashes[&key(&["a.txt"])].len(), 2);
    assert!(!snap.entries.contains_key(&key(&["a.txt"])));
    assert!(snap.entries.contains_key(&key(&["b.txt"])));
    let mut exact = options();
    exact.name_rule = NameRule::ExactCase;
    let snap = run(&volume, &exact).await.unwrap();
    assert!(snap.clashes.is_empty());
    assert_eq!(snap.entries.len(), 3);
}

#[tokio::test]
async fn only_selected_limits_the_top_level() {
    let volume = tree(&[("/a.txt", 1), ("/d/x", 1), ("/other/y", 1)], &["/d", "/other"]).await;
    let mut opts = options();
    opts.only = Some(vec!["D".to_string(), "a.txt".to_string()]);
    let snap = run(&volume, &opts).await.unwrap();
    assert!(snap.entries.contains_key(&key(&["d", "x"])));
    assert!(snap.entries.contains_key(&key(&["a.txt"])));
    assert!(!snap.entries.contains_key(&key(&["other"])));
}

#[tokio::test]
async fn empty_folders_only_with_the_option() {
    let volume = tree(&[("/full/f", 1)], &["/full", "/void", "/void/inner"]).await;
    let mut opts = options();
    opts.empty_folders = false;
    let snap = run(&volume, &opts).await.unwrap();
    assert!(snap.entries.contains_key(&key(&["full"])));
    assert!(!snap.entries.contains_key(&key(&["void"])));
    assert!(!snap.entries.contains_key(&key(&["void", "inner"])));
    let snap = run(&volume, &options()).await.unwrap();
    assert!(snap.entries.contains_key(&key(&["void", "inner"])));
}

#[tokio::test]
async fn without_subfolders_only_the_root_is_listed() {
    let volume = tree(&[("/a.txt", 1), ("/d/x", 1)], &["/d"]).await;
    let mut opts = options();
    opts.subfolders = false;
    let snap = run(&volume, &opts).await.unwrap();
    assert_eq!(snap.entries.len(), 1);
}

#[tokio::test]
async fn cancel_stops_the_walk_with_stopped() {
    let volume = tree(&[("/a.txt", 1)], &[]).await;
    let control = SyncControl::new();
    control.cancel();
    let err = scan(Side::Left, &volume, Path::new("/"), &options(), &control, &|_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, ScanError::Stopped));
}

#[tokio::test]
async fn progress_reports_running_counts() {
    let volume = tree(&[("/a.txt", 4), ("/d/b.txt", 6)], &["/d"]).await;
    let last = std::sync::Mutex::new(None);
    scan(
        Side::Left,
        &volume,
        Path::new("/"),
        &options(),
        &SyncControl::new(),
        &|p| *last.lock().unwrap() = Some(p),
    )
    .await
    .unwrap();
    let p = last.lock().unwrap().take().unwrap();
    assert_eq!((p.folders, p.files, p.bytes), (2, 2, 10));
}

#[tokio::test]
async fn unreadable_nested_folder_marks_ancestors() {
    let volume = FailingListVolume {
        inner: tree(&[("/x/ok.txt", 1), ("/x/y/z", 1)], &["/x", "/x/y"]).await,
        fail_path: PathBuf::from("/x/y"),
        error: denied,
    };
    let snap = run(&volume, &options()).await.unwrap();
    assert_eq!(snap.entries[&key(&["x"])].subtree.unwrap().unreadable, 1);
    assert_eq!(snap.entries[&key(&["x", "y"])].subtree.unwrap().unreadable, 1);
    assert_eq!(snap.entries[&key(&["x", "ok.txt"])].subtree, None);
}

#[tokio::test]
async fn cmdr_sync_folder_is_left_out_and_counted_as_app_owned_at_every_level() {
    let volume = tree(
        &[
            ("/.cmdr-sync/v", 1),
            ("/d/.cmdr-sync/v", 1),
            ("/d/f", 1),
            ("/d/e/.cmdr-sync/v", 1),
            ("/d/e/g", 1),
        ],
        &["/.cmdr-sync", "/d", "/d/.cmdr-sync", "/d/e", "/d/e/.cmdr-sync"],
    )
    .await;
    let snap = run(&volume, &options()).await.unwrap();
    let names: Vec<_> = snap.entries.values().map(|e| e.rel_path.as_str()).collect();
    assert_eq!(names, ["d", "d/e", "d/e/g", "d/f"]);
    let d = snap.entries[&key(&["d"])].subtree.unwrap();
    assert_eq!(d.held_back, 0, "not held back: the scan never offers it");
    assert_eq!(d.app_owned, 2, "its own and `e`'s");
    assert_eq!(snap.entries[&key(&["d", "e"])].subtree.unwrap().app_owned, 1);
}

#[tokio::test]
async fn a_symlink_to_a_folder_is_filtered_as_a_file() {
    let volume = tree(&[("/link", 0)], &[]).await;
    let mut entries = volume.list_directory(Path::new("/"), None).await.unwrap();
    entries[0].is_directory = true;
    entries[0].is_symlink = true;
    let volume = InMemoryVolume::with_entries("t", entries);
    let mut opts = options();
    opts.filter = MaskFilter::parse("| link/", &[]).unwrap();
    // `link/` matches folders only; the link is not one, so it stays.
    let snap = run(&volume, &opts).await.unwrap();
    assert_eq!(snap.entries.len(), 1);
}

fn folder_subtree(snap: &Snapshot, path: &str) -> SubtreeFacts {
    snap.entries[&key(&path.split('/').collect::<Vec<_>>())]
        .subtree
        .unwrap()
}

#[tokio::test]
async fn subtree_now_counts_a_folder_exactly_as_the_scan_did() {
    let volume = tree(
        &[("/a/x.txt", 3), ("/a/b/y.txt", 4), ("/a/.hidden", 1)],
        &["/a", "/a/b", "/a/empty"],
    )
    .await;
    let options = ScanOptions {
        include_hidden: false,
        ..options()
    };
    let snap = run(&volume, &options).await.unwrap();
    let control = SyncControl::new();
    let now = subtree_now(Side::Left, &volume, Path::new("/a"), "a", &options, &control)
        .await
        .unwrap();
    assert_eq!(now, folder_subtree(&snap, "a"));
    assert_eq!((now.files, now.bytes, now.held_back), (2, 7, 1));
}

#[tokio::test]
async fn a_rename_inside_a_folder_changes_its_digest_only() {
    let before = run(&tree(&[("/a/x.txt", 3)], &["/a"]).await, &options()).await.unwrap();
    let after = run(&tree(&[("/a/y.txt", 3)], &["/a"]).await, &options()).await.unwrap();
    let (before, after) = (folder_subtree(&before, "a"), folder_subtree(&after, "a"));
    assert_ne!(before.digest, after.digest);
    assert_eq!(
        SubtreeFacts { digest: 0, ..before },
        SubtreeFacts { digest: 0, ..after }
    );
}

#[tokio::test]
async fn a_change_deep_down_shows_in_every_folder_above() {
    let before = run(
        &tree(&[("/a/b/c/x.txt", 3)], &["/a", "/a/b", "/a/b/c"]).await,
        &options(),
    )
    .await
    .unwrap();
    let after = run(
        &tree(&[("/a/b/c/x.txt", 3), ("/a/b/c/new", 0)], &["/a", "/a/b", "/a/b/c"]).await,
        &options(),
    )
    .await
    .unwrap();
    for folder in ["a", "a/b", "a/b/c"] {
        assert_ne!(
            folder_subtree(&before, folder).digest,
            folder_subtree(&after, folder).digest,
            "{folder}"
        );
    }
}

#[test]
fn scan_and_subtree_now_are_send() {
    // The app runs both on spawned tasks (compare sessions, the engine adapter).
    fn assert_send<T: Send>(_: T) {}
    let volume = InMemoryVolume::new("Send");
    let control = SyncControl::new();
    let options = options();
    assert_send(scan(Side::Left, &volume, Path::new("/"), &options, &control, &|_| {}));
    assert_send(subtree_now(Side::Left, &volume, Path::new("/"), "", &options, &control));
}

#[test]
fn scan_errors_are_std_errors_that_name_no_path() {
    let error = ScanError::RootUnreachable {
        side: Side::Right,
        error: VolumeError::NotFound("/private/thing".into()),
    };
    assert_eq!(error.to_string(), "the Right root wouldn't list");
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(ScanError::Stopped.to_string(), "the scan was stopped");
}
