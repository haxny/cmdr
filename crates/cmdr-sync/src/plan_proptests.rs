//! Planner laws over generated snapshot pairs. `apply_plan` is a pure model of
//! a run (every planned action done, guards ignored), so "re-plan after a run"
//! laws check the planner against itself without any I/O.

use std::collections::BTreeSet;

use proptest::prelude::*;

use crate::test_tree::{Tree, caps};
use crate::*;

const NAMES: [&str; 3] = ["a", "b", "c"];
const MTIMES: [Option<u64>; 5] = [None, Some(100_000), Some(100_001), Some(103_600), Some(200_000)];
const SIZES: [Option<u64>; 4] = [None, Some(0), Some(5), Some(7)];

#[derive(Debug, Clone)]
enum Op {
    File {
        kind: EntryKind,
        size: Option<u64>,
        mtime: Option<u64>,
    },
    Folder,
    HeldBack(u64),
    Unreadable,
    Clash,
}

fn arb_path() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(NAMES.to_vec()), 1..=3).prop_map(|parts| parts.join("/"))
}

fn arb_op() -> impl Strategy<Value = Op> {
    let file = (
        prop::sample::select(vec![EntryKind::File, EntryKind::File, EntryKind::Symlink]),
        prop::sample::select(SIZES.to_vec()),
        prop::sample::select(MTIMES.to_vec()),
    )
        .prop_map(|(kind, size, mtime)| Op::File { kind, size, mtime });
    prop_oneof![
        8 => file,
        2 => Just(Op::Folder),
        1 => (1u64..3).prop_map(Op::HeldBack),
        1 => Just(Op::Unreadable),
        1 => Just(Op::Clash),
    ]
}

fn arb_ops(max: usize) -> impl Strategy<Value = Vec<(String, Op)>> {
    prop::collection::vec((arb_path(), arb_op()), 0..max)
}

fn build(ops: &[(String, Op)], caps: SyncCaps) -> Snapshot {
    let tree = ops
        .iter()
        .fold(Tree::new().with_caps(caps), |tree, (path, op)| match op {
            Op::File { kind, size, mtime } => tree.entry(path, *kind, *size, *mtime),
            Op::Folder => tree.folder(path),
            Op::HeldBack(count) => tree.held_back(path, *count),
            Op::Unreadable => tree.unreadable(path),
            Op::Clash => tree.clash(path),
        });
    tree.build()
}

fn arb_caps() -> impl Strategy<Value = SyncCaps> {
    (1u64..=2).prop_map(|mtime_precision_secs| SyncCaps {
        mtime_precision_secs,
        ..caps()
    })
}

/// Which side, if any, lists empty (an unmounted root): a tenth of cases each.
#[derive(Debug, Clone, Copy)]
enum Blank {
    Neither,
    Left,
    Right,
}

fn arb_blank() -> impl Strategy<Value = Blank> {
    prop_oneof![8 => Just(Blank::Neither), 1 => Just(Blank::Left), 1 => Just(Blank::Right)]
}

/// Shared operations make equal pairs likely; each side's extras then add,
/// replace, hide, or break things.
fn arb_snapshot_pair() -> impl Strategy<Value = (Snapshot, Snapshot)> {
    (arb_ops(10), arb_ops(6), arb_ops(6), arb_caps(), arb_caps(), arb_blank()).prop_map(
        |(shared, left, right, lc, rc, blank)| {
            let side_ops = |extra: &[(String, Op)], blanked: bool| -> Vec<(String, Op)> {
                if blanked {
                    Vec::new()
                } else {
                    shared.iter().chain(extra).cloned().collect()
                }
            };
            let left_ops = side_ops(&left, matches!(blank, Blank::Left));
            let right_ops = side_ops(&right, matches!(blank, Blank::Right));
            (build(&left_ops, lc), build(&right_ops, rc))
        },
    )
}

fn arb_rules() -> impl Strategy<Value = CompareRules> {
    (
        prop::sample::select(vec![CompareBy::DateAndSize, CompareBy::SizeOnly, CompareBy::Content]),
        any::<bool>(),
        0u64..=3,
        any::<bool>(),
        prop::sample::select(vec![0i64, 3600, -3600]),
    )
        .prop_map(
            |(by, ignore_date, tolerance_secs, ignore_hour_shift, right_offset_secs)| CompareRules {
                by,
                ignore_date,
                tolerance_secs,
                ignore_hour_shift,
                right_offset_secs,
            },
        )
}

fn arb_mode() -> impl Strategy<Value = Mode> {
    prop::sample::select(Mode::ALL.to_vec())
}

fn options(mode: Mode, rules: CompareRules) -> PlanOptions {
    PlanOptions { mode, rules }
}

/// Copies the entry at `key` (and everything beneath it) from `source` into
/// `target`, shifting times by `shift` seconds the way the target's clock
/// would list them; creates missing parent folders.
fn copy_into(source: &Snapshot, target: &mut Snapshot, key: &EntryKey, shift: i64) {
    target.entries.retain(|k, _| !k.is_within(key));
    let mut parent = key.parent();
    while let Some(folder) = parent {
        if folder == EntryKey::root() {
            break;
        }
        if !target.entries.contains_key(&folder)
            && let Some(entry) = source.entries.get(&folder)
        {
            let created = Entry {
                subtree: Some(SubtreeFacts::default()),
                ..entry.clone()
            };
            target.entries.insert(folder.clone(), created);
        }
        parent = folder.parent();
    }
    for (k, entry) in source
        .entries
        .range(key.clone()..)
        .take_while(|(k, _)| k.is_within(key))
    {
        let mut copied = entry.clone();
        copied.facts.mtime = entry.facts.mtime.map(|t| t.saturating_add_signed(shift));
        target.entries.insert(k.clone(), copied);
    }
}

fn remove(target: &mut Snapshot, key: &EntryKey) {
    target.entries.retain(|k, _| !k.is_within(key));
}

/// A run, as a pure function: every action in the plan done.
fn apply_plan(plan: &Plan, left: &Snapshot, right: &Snapshot) -> (Snapshot, Snapshot) {
    let offset = plan.rules.right_offset_secs;
    let (mut l, mut r) = (left.clone(), right.clone());
    for item in &plan.items {
        match item.action {
            Action::CopyRight => copy_into(left, &mut r, &item.key, -offset),
            Action::CopyLeft => copy_into(right, &mut l, &item.key, offset),
            Action::RemoveRight => remove(&mut r, &item.key),
            Action::RemoveLeft => remove(&mut l, &item.key),
            Action::Equal | Action::Skip | Action::Undecided => {}
        }
    }
    (l, r)
}

fn removes(action: Action) -> bool {
    matches!(action, Action::RemoveLeft | Action::RemoveRight)
}

fn left_written(action: Action) -> bool {
    matches!(action, Action::CopyLeft | Action::RemoveLeft)
}

fn ignored_after_run(category: Category) -> bool {
    matches!(
        category,
        Category::Unknown | Category::TypeClash | Category::NameClash | Category::Unreadable
    )
}

fn within_unreadable(key: &EntryKey, snap: &Snapshot) -> bool {
    snap.unreadable.keys().any(|u| key.is_within(u))
}

fn non_atomic(entry: &Entry, snap: &Snapshot) -> bool {
    entry.subtree.is_some_and(|s| s.held_back > 0 || s.unreadable > 0)
        || snap.clashes.keys().any(|k| k != &entry.key && k.is_within(&entry.key))
}

/// Entries the plan must leave exactly as they are: everything at or under an
/// unreadable folder, and every non-atomic folder itself.
fn protected(snap: &Snapshot) -> Vec<(EntryKey, Entry)> {
    snap.entries
        .iter()
        .filter(|(k, e)| within_unreadable(k, snap) || non_atomic(e, snap))
        .map(|(k, e)| (k.clone(), e.clone()))
        .collect()
}

fn names(snap: &Snapshot) -> BTreeSet<EntryKey> {
    snap.entries.keys().cloned().collect()
}

proptest! {
    #[test]
    fn plan_is_deterministic((left, right) in arb_snapshot_pair(), mode in arb_mode(), rules in arb_rules()) {
        let options = options(mode, rules);
        let once = plan(&left, &right, None, &options);
        let twice = plan(&left.clone(), &right.clone(), None, &options);
        prop_assert_eq!(&once, &twice);
        for (position, item) in once.items.iter().enumerate() {
            prop_assert_eq!(item.index as usize, position);
        }
        prop_assert!(once.items.windows(2).all(|w| w[0].key < w[1].key));
    }

    #[test]
    fn update_and_both_ways_never_remove((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        for mode in [Mode::Update, Mode::BothWays] {
            let planned = plan(&left, &right, None, &options(mode, rules));
            prop_assert!(planned.items.iter().all(|item| !removes(item.action)), "{:?}", mode);
            let (l, r) = apply_plan(&planned, &left, &right);
            prop_assert!(names(&l).is_superset(&names(&left)));
            prop_assert!(names(&r).is_superset(&names(&right)));
        }
    }

    #[test]
    fn update_never_copies_left((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        let planned = plan(&left, &right, None, &options(Mode::Update, rules));
        prop_assert!(planned.items.iter().all(|item| !left_written(item.action)));
        let (l, _) = apply_plan(&planned, &left, &right);
        prop_assert_eq!(&l.entries, &left.entries);
    }

    #[test]
    fn mirror_never_writes_left((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        let planned = plan(&left, &right, None, &options(Mode::Mirror, rules));
        prop_assert!(planned.items.iter().all(|item| !left_written(item.action)));
        let (l, _) = apply_plan(&planned, &left, &right);
        prop_assert_eq!(&l.entries, &left.entries);
    }

    #[test]
    fn mirror_replanned_after_apply_is_all_equal((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        let options = options(Mode::Mirror, rules);
        let planned = plan(&left, &right, None, &options);
        let (l, r) = apply_plan(&planned, &left, &right);
        let again = plan(&l, &r, None, &options);
        for item in &again.items {
            prop_assert!(
                item.action == Action::Equal || ignored_after_run(item.verdict.category),
                "{:?}: {:?} / {:?}", item.key, item.verdict, item.action
            );
        }
    }

    #[test]
    fn update_replanned_after_apply_copies_nothing((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        let options = options(Mode::Update, rules);
        let planned = plan(&left, &right, None, &options);
        let (l, r) = apply_plan(&planned, &left, &right);
        let again = plan(&l, &r, None, &options);
        prop_assert!(again.items.iter().all(|item| item.action != Action::CopyRight));
    }

    #[test]
    fn both_ways_only_grows_the_union_of_names((left, right) in arb_snapshot_pair(), rules in arb_rules()) {
        let options = options(Mode::BothWays, rules);
        let planned = plan(&left, &right, None, &options);
        let (l, r) = apply_plan(&planned, &left, &right);
        let before: BTreeSet<_> = names(&left).union(&names(&right)).cloned().collect();
        let after: BTreeSet<_> = names(&l).union(&names(&r)).cloned().collect();
        prop_assert!(after.is_superset(&before));
        let again = plan(&l, &r, None, &options);
        for item in &again.items {
            prop_assert!(
                !matches!(item.action, Action::CopyRight | Action::CopyLeft),
                "{:?}: {:?} / {:?}", item.key, item.verdict, item.action
            );
        }
    }

    #[test]
    fn no_default_action_on_unknown_clash_or_unreadable(
        (left, right) in arb_snapshot_pair(), mode in arb_mode(), rules in arb_rules(),
    ) {
        let planned = plan(&left, &right, None, &options(mode, rules));
        for item in &planned.items {
            if ignored_after_run(item.verdict.category) {
                prop_assert_eq!(item.default_action, Action::Skip, "{:?}", item.key);
            }
            if within_unreadable(&item.key, &left) || within_unreadable(&item.key, &right) {
                prop_assert_eq!(item.verdict.category, Category::Unreadable);
            }
            for (side, entry) in [(&left, &item.left), (&right, &item.right)] {
                if let Some(entry) = entry && item.action != Action::Skip && item.action != Action::Equal {
                    prop_assert!(!non_atomic(entry, side), "{:?} acts on a partial folder", item.key);
                }
            }
        }
    }

    #[test]
    fn unreadable_and_non_atomic_entries_survive_a_run(
        (left, right) in arb_snapshot_pair(), mode in arb_mode(), rules in arb_rules(),
    ) {
        let planned = plan(&left, &right, None, &options(mode, rules));
        let (l, r) = apply_plan(&planned, &left, &right);
        for (before, after) in [(&left, &l), (&right, &r)] {
            for (key, entry) in protected(before) {
                prop_assert_eq!(after.entries.get(&key), Some(&entry), "{:?}", key);
            }
        }
    }

    #[test]
    fn older_overwrites_newer_flagged_exactly(
        (left, right) in arb_snapshot_pair(), mode in arb_mode(), rules in arb_rules(),
    ) {
        let planned = plan(&left, &right, None, &options(mode, rules));
        for item in &planned.items {
            let expected = matches!(
                (item.action, item.verdict.category),
                (Action::CopyRight, Category::NewerRight) | (Action::CopyLeft, Category::NewerLeft)
            );
            prop_assert_eq!(item.older_overwrites_newer, expected, "{:?}", item.key);
            if mode == Mode::Mirror && item.verdict.category == Category::NewerRight {
                prop_assert!(item.older_overwrites_newer);
            }
        }
        let report = check(&planned, &left, &right, &GuardSettings::default());
        let flagged = planned.items.iter().filter(|item| item.older_overwrites_newer).count() as u64;
        prop_assert_eq!(report.older_overwrites_newer, flagged);
    }

    #[test]
    fn overrides_never_yield_illegal_actions(
        (left, right) in arb_snapshot_pair(),
        mode in arb_mode(),
        changes in prop::collection::vec((0u32..12, 0usize..7), 0..6),
        left_writes in any::<bool>(),
        right_writes in any::<bool>(),
    ) {
        let planned = plan(&left, &right, None, &options(mode, CompareRules::QUICK));
        let lc = SyncCaps { can_write: left_writes, ..caps() };
        let rc = SyncCaps { can_write: right_writes, ..caps() };
        let all = [
            Action::CopyRight, Action::CopyLeft, Action::RemoveRight, Action::RemoveLeft,
            Action::Equal, Action::Skip, Action::Undecided,
        ];
        let changes: Vec<_> = changes.iter().map(|&(item, a)| ActionChange { item, action: all[a] }).collect();
        let every_legal = changes.iter().all(|change| {
            planned.items.get(change.item as usize)
                .is_some_and(|item| legal_actions(mode, item, &lc, &rc).contains(&change.action))
        });
        match apply_overrides(&planned, &changes, &lc, &rc) {
            Ok(next) => {
                prop_assert!(every_legal);
                for item in &next.items {
                    let changed = changes.iter().any(|change| change.item == item.index);
                    if changed {
                        prop_assert!(legal_actions(mode, item, &lc, &rc).contains(&item.action));
                    }
                    if mode != Mode::CompareOnly {
                        prop_assert!(!(matches!(mode, Mode::Update | Mode::BothWays) && removes(item.action)));
                        prop_assert!(!(matches!(mode, Mode::Update | Mode::Mirror) && left_written(item.action)));
                    }
                }
            }
            Err(_) => prop_assert!(!every_legal),
        }
        for bulk in [
            BulkAction::DefaultDirection, BulkAction::CopyRight, BulkAction::CopyLeft, BulkAction::Reverse,
            BulkAction::Skip, BulkAction::ToggleSkip, BulkAction::RemoveTarget,
        ] {
            let rows: Vec<u32> = (0..14).collect();
            let bulk_changes = bulk_changes(&planned, &rows, bulk, &lc, &rc);
            prop_assert!(apply_overrides(&planned, &bulk_changes, &lc, &rc).is_ok(), "{:?}", bulk);
        }
    }

    #[test]
    fn guards_trip_as_specified(
        (left, right) in arb_snapshot_pair(),
        mode in arb_mode(),
        percent in prop_oneof![prop::sample::select(vec![0u8, 25, 50, 100]), 0u8..=100],
        count in 0u32..4,
        sentinel in prop::option::weighted(0.2, prop::sample::select(NAMES.to_vec())),
        acknowledged in prop::sample::subsequence(vec![Acknowledgement::RemoveAnyway, Acknowledgement::CopyIntoEmpty], 0..=2),
    ) {
        let planned = plan(&left, &right, None, &options(mode, CompareRules::QUICK));
        let settings = GuardSettings { max_remove_percent: percent, max_remove_count: count, sentinel: sentinel.map(str::to_owned) };
        let report = check(&planned, &left, &right, &settings);
        let (l, r) = apply_plan(&planned, &left, &right);
        for (side, before, after) in [(Side::Left, &left, &l), (Side::Right, &right, &r)] {
            // Files the run removes, counted independently from the model.
            let removed = before.entries.iter()
                .filter(|(k, e)| e.facts.kind != EntryKind::Folder && !after.entries.contains_key(*k))
                .count() as u64;
            let target_files = before.file_count();
            let trips = removed > 0
                && (removed * 100 > u64::from(percent) * target_files || (count > 0 && removed > u64::from(count)));
            let block = GuardBlock::MaxRemove { side, removals: removed, target_files };
            prop_assert_eq!(report.blocks.contains(&block), trips, "{:?}", side);
            prop_assert_eq!(report.blocks.contains(&GuardBlock::EmptyRoot { side }), before.is_empty());
        }
        let gated = gate(&planned, &report, &acknowledged);
        if let Some(name) = sentinel {
            let missing = [&left, &right].iter().any(|snap| {
                snap.entries.get(&EntryKey::root().child(name.to_owned()))
                    .is_none_or(|entry| entry.facts.kind == EntryKind::Folder)
            });
            let blocked = report.blocks.iter().any(|b| matches!(b, GuardBlock::SentinelMissing { .. }));
            prop_assert_eq!(blocked, missing);
            prop_assert!(!missing || gated.run.is_empty(), "a missing sentinel holds everything");
        }
        let either_empty = left.is_empty() || right.is_empty();
        for &index in &gated.run {
            let item = &planned.items[index as usize];
            prop_assert!(!(either_empty && removes(item.action)), "a removal ran beside an empty root");
            let into = match item.action {
                Action::CopyRight | Action::RemoveRight => Side::Right,
                _ => Side::Left,
            };
            let max_remove_here = report.blocks.iter()
                .any(|b| matches!(b, GuardBlock::MaxRemove { side, .. } if *side == into));
            if removes(item.action) && max_remove_here {
                prop_assert!(acknowledged.contains(&Acknowledgement::RemoveAnyway));
            }
            if report.blocks.contains(&GuardBlock::EmptyRoot { side: into }) {
                prop_assert!(acknowledged.contains(&Acknowledgement::CopyIntoEmpty));
            }
        }
        let acting = planned.items.iter()
            .filter(|item| !matches!(item.action, Action::Equal | Action::Skip | Action::Undecided))
            .count();
        prop_assert_eq!(gated.run.len() + gated.held.len(), acting, "every action runs or is held");
    }
}
