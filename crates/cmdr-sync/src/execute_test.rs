use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::execute_fake::{Change, FakeEngine, Loc};
use crate::steps_test::{acting, context, index_of, no_trash};
use crate::test_tree::{Tree, caps};
use crate::*;

mod more;

const T: u64 = 1_700_000_000;

pub(super) struct Setup {
    pub(super) left: Snapshot,
    pub(super) right: Snapshot,
    pub(super) plan: Plan,
    pub(super) ctx: RunContext,
}

pub(super) fn setup(mode: Mode, left: Tree, right: Tree, keep: KeepPolicy, right_caps: SyncCaps) -> Setup {
    let left = left.build();
    let right = right.with_caps(right_caps).build();
    let options = PlanOptions {
        mode,
        rules: CompareRules::QUICK,
    };
    let plan = plan(&left, &right, None, &options);
    let ctx = context(keep, caps(), right_caps);
    Setup { left, right, plan, ctx }
}

impl Setup {
    fn engine(&self) -> FakeEngine {
        FakeEngine::new(&self.left, &self.right, &self.ctx)
    }

    async fn run(&self, engine: &FakeEngine, control: &Arc<SyncControl>) -> RunReport {
        execute(&self.plan, &acting(&self.plan), &self.ctx, engine, control, &|_| {}).await
    }

    fn result(&self, report: &RunReport, path: &str) -> RunItemResult {
        let index = index_of(&self.plan, path);
        report
            .results
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, result)| *result)
            .unwrap_or_else(|| panic!("no result for {path}"))
    }
}

pub(super) fn versions(path: &str) -> PathBuf {
    PathBuf::from("/R/.cmdr-sync/versions/2026-11-01 093000").join(path)
}

pub(super) fn file(size: u64, mtime: u64) -> Observed {
    Observed {
        facts: EntryFacts {
            kind: EntryKind::File,
            size: Some(size),
            mtime: Some(mtime),
        },
        subtree: None,
    }
}

/// Every source path any batch handed to the engine.
pub(super) fn touched(engine: &FakeEngine) -> Vec<PathBuf> {
    engine
        .batches()
        .into_iter()
        .flat_map(|batch| match batch {
            EngineBatch::MakeFolders { dirs, .. } => dirs,
            EngineBatch::ClaimFolder { dir, .. } => vec![dir],
            EngineBatch::Copy { sources, .. }
            | EngineBatch::Move { sources, .. }
            | EngineBatch::MoveBack { sources, .. }
            | EngineBatch::Trash { sources, .. } => sources.into_iter().map(|s| s.path).collect(),
        })
        .collect()
}

pub(super) fn assert_clean(engine: &FakeEngine) {
    assert_eq!(engine.violations(), Vec::<String>::new());
    assert_eq!(engine.lost(), Vec::new());
}

#[tokio::test]
async fn changed_source_is_skipped_as_changed_since_compare() {
    let s = setup(
        Mode::Update,
        Tree::new().file("a.txt", 1, T).file("b.txt", 1, T),
        Tree::new(),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Left, "/L/a.txt".into(), file(2, T + 9)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(s.result(&report, "b.txt"), RunItemResult::Done);
    assert!(!touched(&engine).contains(&PathBuf::from("/L/a.txt")));
    assert_clean(&engine);
}

#[tokio::test]
async fn changed_replacement_target_is_skipped() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Right, "/R/a.txt".into(), file(4, T + 50)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    let touched = touched(&engine);
    assert!(!touched.contains(&PathBuf::from("/R/a.txt")) && !touched.contains(&PathBuf::from("/L/a.txt")));
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T + 50)));
    assert_clean(&engine);
}

#[tokio::test]
async fn changed_removal_target_is_skipped() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("k.txt", 1, T),
        Tree::new()
            .file("k.txt", 1, T)
            .file("old.txt", 3, T)
            .file("gone.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Right, "/R/old.txt".into(), file(30, T)))
        .changed(Change::Remove(Loc::Right, "/R/gone.txt".into()));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(s.result(&report, "gone.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.batches(), Vec::new());
    assert_clean(&engine);
}

#[tokio::test]
async fn folder_that_gained_files_is_skipped() {
    // A removal: the folder gained a file nobody saw.
    let s = setup(
        Mode::Mirror,
        Tree::new().file("k.txt", 1, T),
        Tree::new().file("k.txt", 1, T).file("old/y.txt", 2, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Right, "/R/old/surprise.txt".into(), file(1, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "old"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.batches(), Vec::new());
    assert!(engine.get(Loc::Right, "/R/old/surprise.txt").is_some());
    assert_clean(&engine);
    // A whole-folder copy: the source gained a file.
    let s = setup(
        Mode::Update,
        Tree::new().file("k.txt", 1, T).file("new/x.txt", 1, T),
        Tree::new().file("k.txt", 1, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Left, "/L/new/surprise.txt".into(), file(1, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "new"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.batches(), Vec::new());
    assert_clean(&engine);
}

#[tokio::test]
async fn a_file_that_appeared_where_a_copy_lands_is_never_overwritten() {
    let s = setup(
        Mode::Update,
        Tree::new().file("a.txt", 1, T),
        Tree::new(),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s
        .engine()
        .changed(Change::Set(Loc::Right, "/R/a.txt".into(), file(7, T)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.batches(), Vec::new());
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(7, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn split_folder_is_made_before_its_children_land() {
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
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "docs/a.txt"), RunItemResult::Done);
    let batches = engine.batches();
    assert!(matches!(&batches[0], EngineBatch::MakeFolders { dirs, .. } if dirs == &[PathBuf::from("/R/docs")]));
    assert!(matches!(&batches[1], EngineBatch::Copy { dest_dir, .. } if dest_dir == &PathBuf::from("/R/docs")));
    assert_eq!(batches.len(), 2);
    assert!(engine.get(Loc::Right, "/R/docs/a.txt").is_some());
    assert_clean(&engine);
}

#[tokio::test]
async fn failed_copy_holds_removals_in_its_folder_and_restores_the_kept_file() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("d/a.txt", 5, T + 100).file("e/k.txt", 1, T),
        Tree::new()
            .file("d/a.txt", 4, T)
            .file("d/old.txt", 3, T)
            .file("e/k.txt", 1, T)
            .file("e/old2.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let engine = s.engine().failing("/L/d/a.txt");
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "d/a.txt"), RunItemResult::CouldNotDo);
    assert_eq!(s.result(&report, "d/old.txt"), RunItemResult::HeldAfterFailedCopy);
    assert_eq!(s.result(&report, "e/old2.txt"), RunItemResult::Done);
    // The old file is back where it was, untouched.
    assert_eq!(engine.get(Loc::Right, "/R/d/a.txt"), Some(file(4, T)));
    assert_eq!(engine.get(Loc::Right, versions("d/a.txt")), None);
    assert!(engine.get(Loc::Right, "/R/d/old.txt").is_some());
    assert!(engine.get(Loc::Right, versions("e/old2.txt")).is_some());
    assert!(engine.batches().iter().any(|batch| matches!(batch,
        EngineBatch::MoveBack { side: Side::Right, dest_dir, .. } if dest_dir == &PathBuf::from("/R/d"))));
    assert_clean(&engine);
}

#[tokio::test]
async fn failed_copy_under_a_trash_policy_puts_the_original_back() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine().failing("/L/a.txt");
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::CouldNotDo);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert!(engine.trashed().is_empty());
    assert_clean(&engine);
}

#[tokio::test]
async fn source_changed_after_the_keep_step_restores_the_kept_file() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T),
        KeepPolicy::default(),
        no_trash(),
    );
    // Batch 0 claims the versions folder, 1 makes it, 2 sets `a.txt` aside.
    let engine = s
        .engine()
        .after_batch(2, Change::Set(Loc::Left, "/L/a.txt".into(), file(6, T + 200)));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn trash_is_used_when_policy_and_volume_allow() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::Done);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::Done);
    // The replaced file waits beside its copy on the same volume, and goes
    // to the Trash only once the copy landed.
    assert_eq!(
        engine.trashed(),
        vec![
            (Loc::Right, versions("a.txt")),
            (Loc::Right, PathBuf::from("/R/old.txt")),
        ]
    );
    assert!(report.versions_used.is_empty());
    assert_eq!(report.kept_bytes, 0);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(5, T + 100)));
    assert_clean(&engine);
}

#[tokio::test]
async fn versions_folder_is_used_without_a_trash() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100).folder("d"),
        Tree::new().file("a.txt", 4, T).file("d/old.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::Done);
    assert_eq!(s.result(&report, "d/old.txt"), RunItemResult::Done);
    assert!(engine.trashed().is_empty());
    assert_eq!(engine.get(Loc::Right, versions("a.txt")), Some(file(4, T)));
    assert_eq!(engine.get(Loc::Right, versions("d/old.txt")), Some(file(3, T)));
    assert_eq!(report.versions_used, vec![s.ctx.versions_right.clone()]);
    assert_eq!(report.kept_bytes, 7);
    assert_clean(&engine);
}

#[tokio::test]
async fn trash_only_on_a_drive_without_one_touches_nothing() {
    let keep = KeepPolicy {
        place: KeepPlace::TrashOnly,
        ..KeepPolicy::default()
    };
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        keep,
        no_trash(),
    );
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::NoTrash);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::NoTrash);
    assert_eq!(engine.batches(), Vec::new());
    assert_clean(&engine);
}

#[tokio::test]
async fn versions_folder_that_cannot_be_made_keeps_everything_in_place() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let engine = s.engine().failing(versions(""));
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::CouldNotDo);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::CouldNotDo);
    assert_eq!(engine.batches().len(), 1);
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert_clean(&engine);
}

#[tokio::test]
async fn engine_skip_reads_as_changed_since_compare() {
    let s = setup(
        Mode::Update,
        Tree::new().file("a.txt", 1, T),
        Tree::new(),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine().skipping("/L/a.txt");
    let report = s.run(&engine, &SyncControl::new()).await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::ChangedSinceCompare);
    assert_clean(&engine);
}

#[tokio::test]
async fn cancel_between_batches_marks_the_rest_not_started() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100).file("n.txt", 1, T),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let control = SyncControl::new();
    // Batch 0 claims the versions folder, 1 makes it, 2 sets `a.txt` aside,
    // then Cancel.
    let engine = s.engine().cancel_after(2, Arc::clone(&control));
    let report = s.run(&engine, &control).await;
    assert!(report.stopped);
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::NotStarted);
    assert_eq!(s.result(&report, "n.txt"), RunItemResult::NotStarted);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::NotStarted);
    // The kept file went back: a cancel never leaves a replaced file missing.
    assert_eq!(engine.get(Loc::Right, "/R/a.txt"), Some(file(4, T)));
    assert!(!engine.batches().iter().any(|b| matches!(b, EngineBatch::Copy { .. })));
    assert_clean(&engine);
}

#[tokio::test]
async fn a_batch_the_engine_stopped_ends_the_run() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("n.txt", 1, T),
        Tree::new().file("old.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine().stopping_at(0);
    let report = s.run(&engine, &SyncControl::new()).await;
    assert!(report.stopped);
    assert_eq!(s.result(&report, "n.txt"), RunItemResult::NotStarted);
    assert_eq!(s.result(&report, "old.txt"), RunItemResult::NotStarted);
    assert_eq!(engine.batches().len(), 1);
    assert_clean(&engine);
}

#[tokio::test]
async fn pause_holds_the_next_batch_until_released() {
    let s = setup(
        Mode::Update,
        Tree::new().file("a.txt", 1, T),
        Tree::new(),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine();
    let control = SyncControl::new();
    control.set_paused(true);
    let run = s.run(&engine, &control);
    tokio::pin!(run);
    let early = tokio::time::timeout(std::time::Duration::from_millis(20), &mut run).await;
    assert!(early.is_err(), "ran while paused");
    assert!(engine.batches().is_empty());
    control.set_paused(false);
    let report = run.await;
    assert_eq!(s.result(&report, "a.txt"), RunItemResult::Done);
}

#[tokio::test]
async fn progress_counts_steps_items_and_bytes_to_the_end() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100).file("n.txt", 1, T),
        Tree::new().file("a.txt", 4, T).file("old.txt", 3, T),
        KeepPolicy::default(),
        caps(),
    );
    let engine = s.engine();
    let seen = Mutex::new(Vec::new());
    let on_progress = |p: RunProgress| seen.lock().unwrap().push(p);
    let control = SyncControl::new();
    execute(&s.plan, &acting(&s.plan), &s.ctx, &engine, &control, &on_progress).await;
    let seen = seen.into_inner().unwrap();
    let last = seen.last().copied().unwrap();
    assert_eq!(last.steps_done, last.steps_total);
    assert_eq!((last.items_done, last.items_total), (3, 3));
    assert_eq!(last.bytes_done, last.bytes_total);
    assert_eq!(last.bytes_total, 5 + 1 + 3);
    assert!(seen.windows(2).all(|w| w[0].items_done <= w[1].items_done));
}

#[tokio::test]
async fn never_permanently_deletes() {
    let s = setup(
        Mode::Mirror,
        Tree::new().file("a.txt", 5, T + 100).file("d/n.txt", 1, T),
        Tree::new()
            .file("a.txt", 4, T)
            .file("old.txt", 3, T)
            .file("gone/x.txt", 3, T)
            .file("d/y.txt", 3, T),
        KeepPolicy::default(),
        no_trash(),
    );
    let engine = s.engine();
    let report = s.run(&engine, &SyncControl::new()).await;
    assert!(report.results.iter().all(|(_, r)| *r == RunItemResult::Done));
    /* every recorded batch is ClaimFolder | MakeFolders | Copy | Move | MoveBack | Trash */
    for batch in engine.batches() {
        match batch {
            EngineBatch::MakeFolders { .. }
            | EngineBatch::ClaimFolder { .. }
            | EngineBatch::Copy { .. }
            | EngineBatch::Move { .. }
            | EngineBatch::MoveBack { .. }
            | EngineBatch::Trash { .. } => {}
        }
    }
    assert!(engine.get(Loc::Right, versions("gone/x.txt")).is_some());
    assert_clean(&engine);
}

#[test]
fn execute_is_send() {
    fn assert_send<T: Send>(_: T) {}
    let s = setup(Mode::Update, Tree::new(), Tree::new(), KeepPolicy::default(), caps());
    let engine = s.engine();
    let control = SyncControl::new();
    assert_send(execute(&s.plan, &[], &s.ctx, &engine, &control, &|_| {}));
}
