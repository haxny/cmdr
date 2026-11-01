//! Replaced files, holds, versions-folder claims, stops, and cancels.

use std::path::PathBuf;
use std::sync::Arc;

use super::{Setup, assert_clean, file, setup, touched, versions};
use crate::execute_fake::{Change, Loc};
use crate::steps_test::no_trash;
use crate::test_tree::{Tree, caps};
use crate::*;

const T: u64 = 1_700_000_000;

fn mirror_replacement(right_caps: SyncCaps) -> Setup {
    setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        right_caps,
    )
}

fn folder() -> Observed {
    Observed {
        facts: EntryFacts {
            kind: EntryKind::Folder,
            size: None,
            mtime: None,
        },
        subtree: Some(SubtreeFacts::default()),
    }
}

#[tokio::test]
async fn a_mirror_rename_whose_copy_was_skipped_keeps_the_old_name() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("k.txt", 1, T).file("new-name.txt", 5, T),
        Tree::new().file("k.txt", 1, T).file("old-name.txt", 5, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Left, "/L/new-name.txt".into(), file(6, T + 1)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "new-name.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(s.result(&report, "old-name.txt"), RunItemResult::HeldAfterFailedCopy);
    assert_eq!(engine.get(Loc::Right, "/R/old-name.txt"), Some(file(5, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn a_copy_that_could_not_start_holds_removals_beside_it() {
    // Removals go to the Trash, but the replaced file can't be set aside
    // (the dated folder can't be made), so its copy never starts.
    let s = setup(
        Mode::Mirror,
        Tree::new().file("d/a.txt", 5, T + 100).file("e/k.txt", 1, T),
        Tree::new()
            .file("d/a.txt", 4, T)
            .file("d/old.txt", 3, T)
            .file("e/k.txt", 1, T)
            .file("e/old.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine().failing(versions(""));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "d/a.txt"), RunItemResult::CouldNotDo);
    assert_eq!(s.result(&report, "d/old.txt"), RunItemResult::HeldAfterFailedCopy);
    assert_eq!(s.result(&report, "e/old.txt"), RunItemResult::Done);
    assert_eq!(engine.get(Loc::Right, "/R/d/a.txt"), Some(file(4, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn a_keep_the_engine_skips_leaves_the_target_and_skips_the_copy() {
    // `Move` lands with `ConflictResolution::Skip`: the engine says Skipped.
    let s = mirror_replacement(no_trash());
    let engine = s.engine().skipping("/R/a.txt");
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert!(!touched(&engine).contains(&PathBuf::from("/L/a.txt")));
    assert_clean(&engine);
}

#[tokio::test]
async fn a_taken_spot_in_the_versions_folder_is_reported_not_overwritten() {
    let s = mirror_replacement(no_trash());
    // Batch 0 claims the dated folder, batch 1 makes it; then the spot fills.
    let engine = s
        .engine()
        .after_batch(1, Change::Set(Loc::Right, versions("a.txt"), file(9, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::VersionsSpotTaken);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert_eq!(engine.get(Loc::Right, versions("a.txt")), Some(file(9, T)));
    assert_clean(&engine);
    // A removal into a taken spot.
    let s = setup(
        Mode::Mirror,
        Tree::new().file("k.txt", 1, T),
        Tree::new().file("k.txt", 1, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let engine = s
        .engine()
        .after_batch(1, Change::Set(Loc::Right, versions("old.txt"), file(9, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::VersionsSpotTaken);
    assert_eq!(engine.get(Loc::Right, "/R/old.txt"), Some(file(3, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn an_existing_dated_folder_is_never_reused() {
    let s = mirror_replacement(no_trash());
    let engine = s.engine().changed(Change::Set(Loc::Right, versions(""), folder()));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::Done);
    let second = PathBuf::from("/R/.cmdr-sync/versions/2026-11-01 093000 (2)");
    assert_eq!(engine.get(Loc::Right, second.join("a.txt")), Some(file(4, T)));
    assert_eq!(engine.get(Loc::Right, second.join("old.txt")), Some(file(3, T)));
    assert_eq!(report.versions_used[0].dir, second);
    assert_clean(&engine);
}

#[tokio::test]
async fn a_symlink_on_the_way_to_the_versions_folder_refuses_every_keep() {
    let s = mirror_replacement(no_trash());
    let link = Observed {
        facts: EntryFacts {
            kind: EntryKind::Symlink,
            size: Some(10),
            mtime: Some(T),
        },
        subtree: None,
    };
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Right, "/R/.cmdr-sync".into(), link));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::VersionsFolderUnsafe);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::VersionsFolderUnsafe);
    assert_eq!(engine.batches(), Vec::new());
    assert_clean(&engine);
}

#[tokio::test]
async fn a_removal_the_engine_did_before_it_stopped_reads_done() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("k.txt", 1, T),
        Tree::new().file("k.txt", 1, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine().stopping_after_work(0);
    let report = s.run(&engine, &SyncControl::new()).await;
    assert!(report.stopped);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::Done);
    assert_clean(&engine);
}

#[tokio::test]
async fn a_keep_the_engine_did_before_it_stopped_is_put_back() {
    let s = mirror_replacement(no_trash());
    // Batch 2 sets `a.txt` aside and then reports stopped.
    let engine = s.engine().stopping_after_work(2);
    let report = s.run(&engine, &SyncControl::new()).await;
    assert!(report.stopped);
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::NotStarted);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn cancel_while_rechecking_leaves_the_rest_not_started() {
    let s = setup(
        Mode::Update,
        Tree::new().file("a.txt", 1, T).file("b.txt", 1, T),
        Tree::new(),
        KeepPolicy::default(),
        caps(),
    );
    let control = SyncControl::new();
    let engine = s.engine().cancel_on_observe("/L/a.txt", Arc::clone(&control));
    let report = s.run(&engine, &control).await;
    assert!(report.stopped);
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::NotStarted);
    assert_eq!(s.result(&report, "b.txt"), RunItemResult::NotStarted);
    assert_eq!(engine.batches(), Vec::new());
}

#[tokio::test]
async fn folders_made_for_copies_that_all_skipped_are_reported() {
    let s = setup(
        Mode::Update,
        Tree::new()
            .file("k.txt", 1, T)
            .file("docs/a.txt", 1, T)
            .held_back("docs", 1),
        Tree::new().file("k.txt", 1, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Left, "/L/docs/a.txt".into(), file(2, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(report.empty_folders, vec![(Side::Right, PathBuf::from("/R/docs"))]);
    let fine = setup(
        Mode::Update,
        Tree::new()
            .file("k.txt", 1, T)
            .file("docs/a.txt", 1, T)
            .held_back("docs", 1),
        Tree::new().file("k.txt", 1, T),
        KeepPolicy::default(),
        caps(),
    );
    let report = fine.run(&fine.engine(), &SyncControl::new()).await;
    assert!(report.empty_folders.is_empty());
}

#[tokio::test]
async fn a_custom_folder_receives_replaced_files_once_the_copy_landed() {
    let keep = KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: "/Kept".to_owned(),
        },
    };
    let s = setup(
        Mode::Mirror,
        Tree::new().file("d/a.txt", 5, T + 100),
        Tree::new().file("d/a.txt", 4, T),
        keep,
        caps(),
    );
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "d/a.txt"), RunItemResult::Done);
    let kept = PathBuf::from("/Kept/2026-11-01 093000/d/a.txt");
    assert_eq!(engine.get(Loc::Local, &kept), Some(file(4, T)));
    assert_eq!(engine.get(Loc::Right, versions("d/a.txt")), None);
    assert_eq!(engine.get(Loc::Right, "/R/d/a.txt"), Some(file(5, T + 100)));
    assert_eq!(report.versions_used, vec![s.ctx.versions_right.clone()]);
    assert_eq!(report.kept_bytes, 4);
    assert_clean(&engine);
}
