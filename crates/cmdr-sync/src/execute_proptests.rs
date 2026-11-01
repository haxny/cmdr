//! The executor's law: whatever changes on either side between compare and
//! run (or between two batches), nothing that differs from the snapshot is
//! moved, trashed, or copied, nothing lands on top of anything, nothing is
//! lost, and every row that was asked to run is reported.

use std::path::PathBuf;
use std::sync::Arc;

use proptest::prelude::*;

use crate::execute_fake::{Change, FakeEngine, Loc};
use crate::plan_proptests::arb_snapshot_pair;
use crate::steps_test::{acting, context};
use crate::test_tree::caps;
use crate::*;

/// One random edit: which side, which of its entries (by position, wrapping),
/// what happens to it, and after which batch (`None`: before the run).
#[derive(Debug, Clone)]
struct Edit {
    right: bool,
    pick: usize,
    what: u8,
    after: Option<usize>,
}

fn arb_edit() -> impl Strategy<Value = Edit> {
    (any::<bool>(), any::<usize>(), 0u8..5, proptest::option::of(0usize..16)).prop_map(|(right, pick, what, after)| {
        Edit {
            right,
            pick,
            what,
            after,
        }
    })
}

/// How the run gets interrupted, if at all, at which batch (0-based).
#[derive(Debug, Clone, Copy)]
enum Interrupt {
    None,
    /// The person presses Cancel after batch `n` (in-process cancel).
    Cancel(usize),
    /// The engine stops batch `n` before doing anything.
    Stop(usize),
    /// The engine does batch `n`, then reports stopped without answers.
    StopAfterWork(usize),
}

fn arb_interrupt() -> impl Strategy<Value = Interrupt> {
    prop_oneof![
        2 => Just(Interrupt::None),
        1 => (0usize..6).prop_map(Interrupt::Cancel),
        1 => (0usize..6).prop_map(Interrupt::Stop),
        1 => (0usize..6).prop_map(Interrupt::StopAfterWork),
    ]
}

/// The default target-root folder, or a custom one on the local disk.
fn arb_location() -> impl Strategy<Value = VersionsLocation> {
    prop_oneof![
        2 => Just(KeepPolicy::default().location),
        1 => Just(VersionsLocation::Custom { template: "/Kept/{profile}".to_owned() }),
    ]
}

fn arb_mode() -> impl Strategy<Value = Mode> {
    prop::sample::select(vec![Mode::Mirror, Mode::Mirror, Mode::BothWays, Mode::Update])
}

fn arb_place() -> impl Strategy<Value = KeepPlace> {
    prop::sample::select(vec![
        KeepPlace::TrashElseVersions,
        KeepPlace::VersionsAlways,
        KeepPlace::TrashOnly,
    ])
}

/// Turns an edit into a change of an entry the snapshot has, or of the spot
/// where the other side's entry would land (something new appearing there).
fn change_of(edit: &Edit, left: &Snapshot, right: &Snapshot) -> Option<Change> {
    let (snap, other, loc, root) = if edit.right {
        (right, left, Loc::Right, "/R")
    } else {
        (left, right, Loc::Left, "/L")
    };
    let pool: Vec<&Entry> = if edit.what == 4 {
        other.entries.values().collect()
    } else {
        snap.entries.values().collect()
    };
    let entry = pool.get(edit.pick % pool.len().max(1))?;
    let path = PathBuf::from(root).join(&entry.rel_path);
    let observed = Observed {
        facts: entry.facts,
        subtree: entry.subtree,
    };
    let bumped = |facts: EntryFacts, size: u64, mtime: u64| Observed {
        facts: EntryFacts {
            size: facts.size.map(|s| s + size).or(Some(size)),
            mtime: facts.mtime.map(|m| m + mtime).or(Some(mtime)),
            ..facts
        },
        subtree: None,
    };
    Some(match edit.what {
        0 if entry.facts.kind == EntryKind::Folder => {
            Change::Set(loc, path.join("appeared.txt"), bumped(entry.facts, 1, 1))
        }
        0 => Change::Set(loc, path, bumped(entry.facts, 1, 0)),
        1 if entry.facts.kind == EntryKind::Folder => Change::Set(loc, path.join("zz"), bumped(entry.facts, 0, 0)),
        1 => Change::Set(loc, path, bumped(entry.facts, 0, 7)),
        2 => Change::Remove(loc, path),
        3 => Change::Set(loc, path, observed),
        _ => Change::Set(loc, path, bumped(entry.facts, 3, 3)),
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]
    #[test]
    fn executor_never_touches_what_changed_and_never_loses_anything(
        (left, right) in arb_snapshot_pair(),
        mode in arb_mode(),
        place in arb_place(),
        left_trash in any::<bool>(),
        right_trash in any::<bool>(),
        edits in prop::collection::vec(arb_edit(), 0..6),
        location in arb_location(),
        interrupt in arb_interrupt(),
    ) {
        let options = PlanOptions { mode, rules: CompareRules::QUICK };
        let plan = plan(&left, &right, None, &options);
        let keep = KeepPolicy { place, location };
        let left_caps = SyncCaps { has_trash: left_trash, ..caps() };
        let right_caps = SyncCaps { has_trash: right_trash, ..caps() };
        let ctx = context(keep, left_caps, right_caps);
        let mut engine = FakeEngine::new(&left, &right, &ctx);
        for edit in &edits {
            let Some(change) = change_of(edit, &left, &right) else { continue };
            engine = match edit.after {
                None => engine.changed(change),
                Some(n) => engine.after_batch(n, change),
            };
        }
        let run = acting(&plan);
        let control = SyncControl::new();
        engine = match interrupt {
            Interrupt::None => engine,
            Interrupt::Cancel(n) => engine.cancel_after(n, Arc::clone(&control)),
            Interrupt::Stop(n) => engine.stopping_at(n),
            Interrupt::StopAfterWork(n) => engine.stopping_after_work(n),
        };
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let report = runtime.block_on(execute(&plan, &run, &ctx, &engine, &control, &|_| {}));
        prop_assert_eq!(engine.violations(), Vec::<String>::new());
        prop_assert_eq!(engine.lost(), Vec::new());
        let reported: Vec<u32> = report.results.iter().map(|(index, _)| *index).collect();
        prop_assert_eq!(reported, run);
        // Interrupted or not: a row that didn't end `Done` (or `KeptNotReplaced`,
        // which says so) left what it would replace or remove where it was.
        for (index, result) in &report.results {
            let item = &plan.items[*index as usize];
            let Some(side) = item.action.target() else { continue };
            let Some(old) = item.entry(side) else { continue };
            let (loc, root) = match side { Side::Left => (Loc::Left, "/L"), Side::Right => (Loc::Right, "/R") };
            let away = engine.moved_away(loc, PathBuf::from(root).join(&old.rel_path));
            prop_assert!(
                matches!(result, RunItemResult::Done | RunItemResult::KeptNotReplaced) || !away,
                "row {} reads {:?} but its old file is gone", index, result
            );
        }
        // Only an interrupt stops a run, and only a stopped run leaves rows unreached.
        prop_assert!(!report.stopped || !matches!(interrupt, Interrupt::None));
        if !report.stopped {
            prop_assert!(report.results.iter().all(|(_, result)| *result != RunItemResult::NotStarted));
        }
        if matches!(interrupt, Interrupt::None) && edits.is_empty() {
            prop_assert!(report.results.iter().all(|(_, result)| matches!(
                result,
                RunItemResult::Done | RunItemResult::NoTrash
            )), "{:?}", report.results);
        }
    }
}
