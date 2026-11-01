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

fn items(rows: &[ViewRow]) -> Vec<u32> {
    rows.iter()
        .filter_map(|row| match row {
            ViewRow::Item { index } => Some(*index),
            ViewRow::Folder { .. } => None,
        })
        .collect()
}

#[test]
fn default_filters_show_all_but_equal() {
    let filters = ShowFilters::default();
    assert!(filters.to_right && filters.to_left && filters.different && filters.unknown);
    assert!(filters.duplicates && filters.singles);
    assert!(!filters.equal);
    assert!(!filters.warnings_only);
}

#[test]
fn row_class_follows_the_action() {
    let left =
        Tree::new()
            .file("eq", 1, T)
            .file("new", 1, T)
            .file("diff", 1, T)
            .entry("unk", EntryKind::File, Some(1), None);
    let right =
        Tree::new()
            .file("eq", 1, T)
            .file("old", 1, T)
            .file("diff", 2, T)
            .entry("unk", EntryKind::File, None, None);
    let mirror = plan_of(Mode::Mirror, left.clone(), right.clone());
    let class = |plan: &Plan, path| row_class(&plan.items[index(plan, path) as usize]);
    assert_eq!(class(&mirror, "eq"), RowClass::Equal);
    assert_eq!(class(&mirror, "new"), RowClass::ToRight);
    assert_eq!(
        class(&mirror, "old"),
        RowClass::ToLeft,
        "a removal on the right shows as ←"
    );
    assert_eq!(class(&mirror, "unk"), RowClass::Unknown);
    let both = plan_of(Mode::BothWays, left, right);
    assert_eq!(class(&both, "diff"), RowClass::Different);
    assert_eq!(class(&both, "old"), RowClass::ToLeft);
}

#[test]
fn equal_rows_hidden_by_default_and_counted() {
    let left = Tree::new().file("a", 1, T).file("b", 1, T).file("c", 1, T);
    let right = Tree::new().file("a", 1, T).file("b", 1, T);
    let plan = plan_of(Mode::Mirror, left, right);
    let rows = view_rows(&plan, &ShowFilters::default());
    assert_eq!(items(&rows), vec![index(&plan, "c")]);
    let summary = summarize(&plan, &ShowFilters::default());
    assert_eq!(summary.equal, 2);
    assert_eq!(
        summary.hidden_actions, 0,
        "equal rows do nothing, so hiding them hides no action"
    );
    assert_eq!(summary.unique_left, 1);
}

#[test]
fn hiding_a_row_keeps_its_action_and_counts_it_hidden() {
    let plan = plan_of(Mode::Mirror, Tree::new().file("a", 7, T), Tree::new().file("z", 1, T));
    let filters = ShowFilters {
        to_right: false,
        ..ShowFilters::default()
    };
    let rows = view_rows(&plan, &filters);
    assert_eq!(items(&rows), vec![index(&plan, "z")]);
    assert_eq!(plan.items[index(&plan, "a") as usize].action, Action::CopyRight);
    let summary = summarize(&plan, &filters);
    assert_eq!(summary.hidden_actions, 1);
    assert_eq!(summary.into_right.copy_files, 1);
    assert_eq!(summary.into_right.copy_bytes, 7);
    assert_eq!(summary.into_right.removals, 1);
}

#[test]
fn presence_and_warning_filters() {
    let left = Tree::new().file("pair", 1, T).file("solo", 1, T).file("older", 1, T);
    let right = Tree::new().file("pair", 2, T - 100).file("older", 1, T + 100);
    let plan = plan_of(Mode::Mirror, left, right);
    let singles_off = ShowFilters {
        singles: false,
        ..ShowFilters::default()
    };
    let mut want = vec![index(&plan, "older"), index(&plan, "pair")];
    want.sort_unstable();
    assert_eq!(items(&view_rows(&plan, &singles_off)), want);
    let duplicates_off = ShowFilters {
        duplicates: false,
        ..ShowFilters::default()
    };
    assert_eq!(items(&view_rows(&plan, &duplicates_off)), vec![index(&plan, "solo")]);
    let warnings = ShowFilters {
        warnings_only: true,
        ..ShowFilters::default()
    };
    assert_eq!(items(&view_rows(&plan, &warnings)), vec![index(&plan, "older")]);
}

#[test]
fn folder_rows_number_n_of_n() {
    let left = Tree::new()
        .file("top", 1, T)
        .file("a/x", 1, T)
        .file("b/y", 1, T)
        .file("b/z", 1, T);
    let right = Tree::new().folder("a").folder("b");
    let plan = plan_of(Mode::Mirror, left, right);
    let rows = view_rows(&plan, &ShowFilters::default());
    assert_eq!(
        rows,
        vec![
            ViewRow::Item {
                index: index(&plan, "top")
            },
            ViewRow::Folder {
                key: key("a"),
                rel_dir: "a".to_owned(),
                ordinal: 1,
                of: 2
            },
            ViewRow::Item {
                index: index(&plan, "a/x")
            },
            ViewRow::Folder {
                key: key("b"),
                rel_dir: "b".to_owned(),
                ordinal: 2,
                of: 2
            },
            ViewRow::Item {
                index: index(&plan, "b/y")
            },
            ViewRow::Item {
                index: index(&plan, "b/z")
            },
        ]
    );
}

#[test]
fn a_folder_with_no_visible_rows_has_no_header() {
    let left = Tree::new().file("a/x", 1, T).file("b/y", 1, T);
    let right = Tree::new().file("a/x", 1, T).folder("b");
    let plan = plan_of(Mode::Mirror, left, right);
    let rows = view_rows(&plan, &ShowFilters::default());
    assert_eq!(rows.len(), 2);
    assert!(matches!(&rows[0], ViewRow::Folder { ordinal: 1, of: 1, rel_dir, .. } if rel_dir == "b"));
}

#[test]
fn summary_counts_every_kind() {
    let left = Tree::new()
        .file("eq", 1, T)
        .file("new", 3, T)
        .file("diff", 4, T + 100)
        .clash("cl");
    let right = Tree::new()
        .file("eq", 1, T)
        .file("gone", 5, T)
        .file("diff", 2, T)
        .file("cl", 1, T);
    let plan = plan_of(Mode::Mirror, left, right);
    let plan = apply_overrides(
        &plan,
        &[ActionChange {
            item: index(&plan, "gone"),
            action: Action::Skip,
        }],
    )
    .unwrap();
    let summary = summarize(&plan, &ShowFilters::default());
    assert_eq!(
        summary,
        PlanSummary {
            into_left: SideTotals::default(),
            into_right: SideTotals {
                copy_files: 1,
                copy_bytes: 3,
                replace_files: 1,
                replace_bytes: 4,
                removals: 0,
                removed_folders: 0,
            },
            skipped: 2,
            undecided: 0,
            equal: 1,
            different: 1,
            unique_left: 1,
            unique_right: 1,
            not_comparable: 1,
            hidden_actions: 0,
            changed_by_you: 1,
        }
    );
}

#[test]
fn removed_folders_count_as_folders_and_their_files_as_files() {
    let left = Tree::new().file("keep", 1, T);
    let right = Tree::new()
        .file("keep", 1, T)
        .folder("empty")
        .file("old/a", 2, T)
        .file("old/b", 3, T);
    let summary = summarize(&plan_of(Mode::Mirror, left, right), &ShowFilters::default());
    assert_eq!(summary.into_right.removals, 2, "files");
    assert_eq!(summary.into_right.removed_folders, 2, "`empty` and `old`");
}

#[test]
fn clash_type_clash_and_unreadable_rows_are_not_comparable() {
    let left = Tree::new().clash("c").file("t", 1, T).unreadable("u");
    let right = Tree::new().file("c", 1, T).file("t/x", 1, T).file("u/y", 1, T);
    let summary = summarize(&plan_of(Mode::CompareOnly, left, right), &ShowFilters::default());
    assert_eq!(summary.not_comparable, 3);
    assert_eq!(
        (summary.different, summary.unique_left, summary.unique_right),
        (0, 0, 0)
    );
}

#[test]
fn chosen_unknown_counts_only_question_rows_given_an_action() {
    let chosen = |plan: &Plan| plan.items.iter().filter(|item| is_chosen_unknown(item)).count();
    let left = Tree::new()
        .entry("unk", EntryKind::File, Some(1), None)
        .entry("left-alone", EntryKind::File, Some(1), None)
        .file("new", 1, T);
    let right = Tree::new()
        .entry("unk", EntryKind::File, None, None)
        .entry("left-alone", EntryKind::File, None, None);
    let plan = plan_of(Mode::Mirror, left, right);
    assert_eq!(chosen(&plan), 0, "a `?` row waits for a choice");
    let plan = apply_overrides(
        &plan,
        &[ActionChange {
            item: index(&plan, "unk"),
            action: Action::CopyRight,
        }],
    )
    .unwrap();
    assert_eq!(chosen(&plan), 1, "the one you chose, not the new copy");
}
