use crate::test_tree::{Tree, key};
use crate::*;

const T: u64 = 1_700_000_000;

fn plan_of(mode: Mode, left: Tree, right: Tree) -> Plan {
    let options = PlanOptions {
        mode,
        rules: CompareRules::QUICK,
    };
    plan(&left.build(), &right.build(), None, &options)
}

fn index(plan: &Plan, path: &str) -> u32 {
    let wanted = key(path);
    plan.items.iter().find(|item| item.key == wanted).expect("item").index
}

fn change(item: u32, action: Action) -> ActionChange {
    ActionChange { item, action }
}

#[test]
fn removal_is_illegal_in_update_and_both_ways() {
    for mode in [Mode::Update, Mode::BothWays] {
        let plan = plan_of(mode, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
        let b = index(&plan, "b");
        let r = apply_overrides(&plan, &[change(b, Action::RemoveRight)]);
        assert!(
            matches!(r, Err(OverrideRefusal::IllegalForMode { item, action: Action::RemoveRight }) if item == b),
            "{mode:?}: {r:?}"
        );
        let a = index(&plan, "a");
        let r = apply_overrides(&plan, &[change(a, Action::RemoveLeft)]);
        assert!(matches!(r, Err(OverrideRefusal::IllegalForMode { .. })), "{mode:?}");
    }
}

#[test]
fn update_and_mirror_never_write_left() {
    for mode in [Mode::Update, Mode::Mirror] {
        let plan = plan_of(mode, Tree::new().file("a", 1, T), Tree::new().file("a", 1, T + 100));
        let a = index(&plan, "a");
        for action in [Action::CopyLeft, Action::RemoveLeft] {
            let r = apply_overrides(&plan, &[change(a, action)]);
            assert!(
                matches!(r, Err(OverrideRefusal::IllegalForMode { .. })),
                "{mode:?} {action:?}"
            );
        }
    }
}

#[test]
fn write_to_a_read_only_side_is_refused() {
    // The plan's own caps decide; callers can't pass others.
    let read_only = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new().read_only());
    let a = index(&read_only, "a");
    let r = apply_overrides(&read_only, &[change(a, Action::CopyRight)]);
    assert!(
        matches!(r, Err(OverrideRefusal::SideReadOnly { side: Side::Right, .. })),
        "{r:?}"
    );
    assert!(!legal_actions(&read_only, &read_only.items[a as usize]).contains(&Action::CopyRight));
    let plan = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new());
    let ok = apply_overrides(&plan, &[change(a, Action::CopyRight)]).unwrap();
    assert_eq!(ok.items[a as usize].action, Action::CopyRight);
    assert!(ok.items[a as usize].changed_by_you());
}

#[test]
fn copying_from_the_missing_side_is_refused() {
    let plan = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new());
    let a = index(&plan, "a");
    let r = apply_overrides(&plan, &[change(a, Action::CopyLeft)]);
    assert!(
        matches!(r, Err(OverrideRefusal::NothingToCopy { side: Side::Right, .. })),
        "{r:?}"
    );
    let r = apply_overrides(&plan, &[change(99, Action::Skip)]);
    assert!(matches!(r, Err(OverrideRefusal::NoSuchItem { item: 99 })));
}

#[test]
fn clash_and_unreadable_rows_take_skip_only() {
    let left = Tree::new().clash("x").unreadable("u");
    let right = Tree::new().file("x", 1, T).file("u/a", 1, T);
    let plan = plan_of(Mode::CompareOnly, left, right);
    for path in ["x", "u"] {
        let i = index(&plan, path);
        assert_eq!(legal_actions(&plan, &plan.items[i as usize]), vec![Action::Skip]);
        let r = apply_overrides(&plan, &[change(i, Action::CopyLeft)]);
        assert!(
            matches!(r, Err(OverrideRefusal::IllegalForCategory { item, action: Action::CopyLeft }) if item == i),
            "{r:?}"
        );
    }
}

#[test]
fn one_refusal_rejects_the_whole_batch() {
    let plan = plan_of(Mode::Update, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let (a, b) = (index(&plan, "a"), index(&plan, "b"));
    let r = apply_overrides(&plan, &[change(a, Action::Skip), change(b, Action::RemoveRight)]);
    assert!(r.is_err());
    assert_eq!(
        plan.items[a as usize].action,
        Action::CopyRight,
        "the input plan is untouched"
    );
}

#[test]
fn reverse_flips_copy_direction_and_skips_what_cannot_flip() {
    let left = Tree::new().file("a", 1, T + 100).file("b", 1, T).file("c", 1, T);
    let right = Tree::new().file("a", 1, T).file("b", 1, T + 100);
    let plan = plan_of(Mode::BothWays, left.clone(), right.clone());
    let all: Vec<u32> = plan.items.iter().map(|item| item.index).collect();
    let changes = bulk_changes(&plan, &all, BulkAction::Reverse);
    let (a, b) = (index(&plan, "a"), index(&plan, "b"));
    assert_eq!(changes, vec![change(a, Action::CopyLeft), change(b, Action::CopyRight)]);
    let flipped = apply_overrides(&plan, &changes).unwrap();
    assert_eq!(flipped.items[a as usize].action, Action::CopyLeft);

    let mirror = plan_of(Mode::Mirror, left, right);
    let all: Vec<u32> = mirror.items.iter().map(|item| item.index).collect();
    assert!(bulk_changes(&mirror, &all, BulkAction::Reverse).is_empty());
}

#[test]
fn bulk_changes_drop_illegal_rows_silently() {
    let plan = plan_of(Mode::Update, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let all: Vec<u32> = plan.items.iter().map(|item| item.index).collect();
    assert!(bulk_changes(&plan, &all, BulkAction::CopyLeft).is_empty());

    let mirror = plan_of(Mode::Mirror, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let (a, b) = (index(&mirror, "a"), index(&mirror, "b"));
    let skipped = apply_overrides(&mirror, &[change(b, Action::Skip)]).unwrap();
    let changes = bulk_changes(&skipped, &[a, b, 42], BulkAction::RemoveTarget);
    assert_eq!(
        changes,
        vec![change(b, Action::RemoveRight)],
        "a has no target; 42 isn't a row"
    );
    assert!(apply_overrides(&skipped, &changes).is_ok());
}

#[test]
fn bulk_outcome_says_why_each_row_was_left_alone() {
    // Update: copies to the left are illegal for the mode; a left-only row has nothing on the right.
    let left = Tree::new().file("a", 1, T).file("both", 2, T);
    let right = Tree::new().file("b", 1, T).file("both", 1, T);
    let plan = plan_of(Mode::Update, left, right);
    let (a, b, both) = (index(&plan, "a"), index(&plan, "b"), index(&plan, "both"));
    let outcome = bulk_outcome(&plan, &[a, b, both, 42], BulkAction::CopyLeft);
    assert!(outcome.changes.is_empty());
    let total: u32 = outcome.skipped.iter().map(|rows| rows.count).sum();
    assert_eq!(total, 3, "42 isn't a row: {outcome:?}");
    assert!(outcome.skipped.iter().any(|rows| rows.reason
        == RowSkipReason::IllegalForMode {
            action: Action::CopyLeft
        }));

    // Reverse on a one-sided copy, Space on an equal row.
    let mirror = plan_of(
        Mode::Mirror,
        Tree::new().file("n", 1, T).file("e", 1, T),
        Tree::new().file("e", 1, T),
    );
    let (n, e) = (index(&mirror, "n"), index(&mirror, "e"));
    let reversed = bulk_outcome(&mirror, &[n], BulkAction::Reverse);
    assert_eq!(
        reversed.skipped,
        vec![SkippedRows {
            reason: RowSkipReason::CantReverse,
            count: 1
        }]
    );
    let toggled = bulk_outcome(&mirror, &[e], BulkAction::ToggleSkip);
    assert_eq!(toggled.skipped[0].reason, RowSkipReason::NothingToToggle);
    // A row that already has the action is neither changed nor refused.
    let same = bulk_outcome(&mirror, &[n], BulkAction::CopyRight);
    assert!(same.changes.is_empty() && same.skipped.is_empty());
}

#[test]
fn toggle_skip_and_default_direction_round_trip() {
    let plan = plan_of(Mode::Mirror, Tree::new().file("a", 1, T), Tree::new().file("a", 1, T));
    let mirror = plan_of(Mode::Mirror, Tree::new().file("n", 1, T), Tree::new());
    let n = index(&mirror, "n");
    let off = bulk_changes(&mirror, &[n], BulkAction::ToggleSkip);
    assert_eq!(off, vec![change(n, Action::Skip)]);
    let skipped = apply_overrides(&mirror, &off).unwrap();
    let on = bulk_changes(&skipped, &[n], BulkAction::ToggleSkip);
    assert_eq!(on, vec![change(n, Action::CopyRight)]);
    let reset = bulk_changes(&skipped, &[n], BulkAction::DefaultDirection);
    assert_eq!(reset, on);
    // An equal row has nothing to toggle.
    let a = index(&plan, "a");
    assert!(bulk_changes(&plan, &[a], BulkAction::ToggleSkip).is_empty());
}

#[test]
fn equal_and_undecided_are_category_refusals() {
    let plan = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new());
    let a = index(&plan, "a");
    for action in [Action::Equal, Action::Undecided] {
        let r = apply_overrides(&plan, &[change(a, action)]);
        assert!(
            matches!(r, Err(OverrideRefusal::IllegalForCategory { .. })),
            "{action:?}: {r:?}"
        );
    }
}

#[test]
fn bulk_copy_and_remove_reach_selected_unknown_rows() {
    // Both sizes unknown and no times: `?`. Selecting it and pressing ⌘L is a choice.
    let left = Tree::new().entry("u", EntryKind::File, None, None);
    let right = Tree::new().entry("u", EntryKind::File, None, None);
    let plan = plan_of(Mode::Mirror, left, right);
    let u = index(&plan, "u");
    assert_eq!(plan.items[u as usize].verdict.category, Category::Unknown);
    assert_eq!(plan.items[u as usize].action, Action::Skip);
    assert_eq!(
        bulk_changes(&plan, &[u], BulkAction::CopyRight),
        vec![change(u, Action::CopyRight)]
    );
    assert_eq!(
        bulk_changes(&plan, &[u], BulkAction::RemoveTarget),
        vec![change(u, Action::RemoveRight)]
    );
}
