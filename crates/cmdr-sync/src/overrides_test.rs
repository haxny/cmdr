use crate::test_tree::{Tree, caps, key};
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
        let r = apply_overrides(&plan, &[change(b, Action::RemoveRight)], &caps(), &caps());
        assert!(
            matches!(r, Err(OverrideRefusal::IllegalForMode { item, action: Action::RemoveRight }) if item == b),
            "{mode:?}: {r:?}"
        );
        let a = index(&plan, "a");
        let r = apply_overrides(&plan, &[change(a, Action::RemoveLeft)], &caps(), &caps());
        assert!(matches!(r, Err(OverrideRefusal::IllegalForMode { .. })), "{mode:?}");
    }
}

#[test]
fn update_and_mirror_never_write_left() {
    for mode in [Mode::Update, Mode::Mirror] {
        let plan = plan_of(mode, Tree::new().file("a", 1, T), Tree::new().file("a", 1, T + 100));
        let a = index(&plan, "a");
        for action in [Action::CopyLeft, Action::RemoveLeft] {
            let r = apply_overrides(&plan, &[change(a, action)], &caps(), &caps());
            assert!(
                matches!(r, Err(OverrideRefusal::IllegalForMode { .. })),
                "{mode:?} {action:?}"
            );
        }
    }
}

#[test]
fn write_to_a_read_only_side_is_refused() {
    let plan = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new());
    let a = index(&plan, "a");
    let read_only = SyncCaps {
        can_write: false,
        ..caps()
    };
    let r = apply_overrides(&plan, &[change(a, Action::CopyRight)], &caps(), &read_only);
    assert!(
        matches!(r, Err(OverrideRefusal::SideReadOnly { side: Side::Right, .. })),
        "{r:?}"
    );
    let ok = apply_overrides(&plan, &[change(a, Action::CopyRight)], &caps(), &caps()).unwrap();
    assert_eq!(ok.items[a as usize].action, Action::CopyRight);
    assert!(ok.items[a as usize].changed_by_you());
}

#[test]
fn copying_from_the_missing_side_is_refused() {
    let plan = plan_of(Mode::CompareOnly, Tree::new().file("a", 1, T), Tree::new());
    let a = index(&plan, "a");
    let r = apply_overrides(&plan, &[change(a, Action::CopyLeft)], &caps(), &caps());
    assert!(
        matches!(r, Err(OverrideRefusal::NothingToCopy { side: Side::Right, .. })),
        "{r:?}"
    );
    let r = apply_overrides(&plan, &[change(99, Action::Skip)], &caps(), &caps());
    assert!(matches!(r, Err(OverrideRefusal::NoSuchItem { item: 99 })));
}

#[test]
fn clash_and_unreadable_rows_take_skip_only() {
    let left = Tree::new().clash("x").unreadable("u");
    let right = Tree::new().file("x", 1, T).file("u/a", 1, T);
    let plan = plan_of(Mode::CompareOnly, left, right);
    for path in ["x", "u"] {
        let i = index(&plan, path);
        assert_eq!(
            legal_actions(Mode::CompareOnly, &plan.items[i as usize], &caps(), &caps()),
            vec![Action::Skip]
        );
        let r = apply_overrides(&plan, &[change(i, Action::CopyLeft)], &caps(), &caps());
        assert!(matches!(r, Err(OverrideRefusal::IllegalForMode { .. })));
    }
}

#[test]
fn one_refusal_rejects_the_whole_batch() {
    let plan = plan_of(Mode::Update, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let (a, b) = (index(&plan, "a"), index(&plan, "b"));
    let r = apply_overrides(
        &plan,
        &[change(a, Action::Skip), change(b, Action::RemoveRight)],
        &caps(),
        &caps(),
    );
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
    let changes = bulk_changes(&plan, &all, BulkAction::Reverse, &caps(), &caps());
    let (a, b) = (index(&plan, "a"), index(&plan, "b"));
    assert_eq!(changes, vec![change(a, Action::CopyLeft), change(b, Action::CopyRight)]);
    let flipped = apply_overrides(&plan, &changes, &caps(), &caps()).unwrap();
    assert_eq!(flipped.items[a as usize].action, Action::CopyLeft);

    let mirror = plan_of(Mode::Mirror, left, right);
    let all: Vec<u32> = mirror.items.iter().map(|item| item.index).collect();
    assert!(bulk_changes(&mirror, &all, BulkAction::Reverse, &caps(), &caps()).is_empty());
}

#[test]
fn bulk_changes_drop_illegal_rows_silently() {
    let plan = plan_of(Mode::Update, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let all: Vec<u32> = plan.items.iter().map(|item| item.index).collect();
    assert!(bulk_changes(&plan, &all, BulkAction::CopyLeft, &caps(), &caps()).is_empty());

    let mirror = plan_of(Mode::Mirror, Tree::new().file("a", 1, T), Tree::new().file("b", 1, T));
    let (a, b) = (index(&mirror, "a"), index(&mirror, "b"));
    let skipped = apply_overrides(&mirror, &[change(b, Action::Skip)], &caps(), &caps()).unwrap();
    let changes = bulk_changes(&skipped, &[a, b, 42], BulkAction::RemoveTarget, &caps(), &caps());
    assert_eq!(
        changes,
        vec![change(b, Action::RemoveRight)],
        "a has no target; 42 isn't a row"
    );
    assert!(apply_overrides(&skipped, &changes, &caps(), &caps()).is_ok());
}

#[test]
fn toggle_skip_and_default_direction_round_trip() {
    let plan = plan_of(Mode::Mirror, Tree::new().file("a", 1, T), Tree::new().file("a", 1, T));
    let mirror = plan_of(Mode::Mirror, Tree::new().file("n", 1, T), Tree::new());
    let n = index(&mirror, "n");
    let off = bulk_changes(&mirror, &[n], BulkAction::ToggleSkip, &caps(), &caps());
    assert_eq!(off, vec![change(n, Action::Skip)]);
    let skipped = apply_overrides(&mirror, &off, &caps(), &caps()).unwrap();
    let on = bulk_changes(&skipped, &[n], BulkAction::ToggleSkip, &caps(), &caps());
    assert_eq!(on, vec![change(n, Action::CopyRight)]);
    let reset = bulk_changes(&skipped, &[n], BulkAction::DefaultDirection, &caps(), &caps());
    assert_eq!(reset, on);
    // An equal row has nothing to toggle.
    let a = index(&plan, "a");
    assert!(bulk_changes(&plan, &[a], BulkAction::ToggleSkip, &caps(), &caps()).is_empty());
}
