use crate::test_tree::Tree;
use crate::*;

const T: u64 = 1_700_000_000;

fn view() -> (Plan, Vec<ViewRow>) {
    let options = PlanOptions {
        mode: Mode::BothWays,
        rules: CompareRules::QUICK,
    };
    let tree = |size| {
        Tree::new()
            .file("root.txt", size, T)
            .file("a/one.txt", size, T)
            .file("a/Two.txt", size, T)
            .file("b/three.txt", size, T)
    };
    let plan = plan(&tree(1).build(), &tree(2).build(), None, &options);
    let rows = view_rows(&plan, &ShowFilters::default());
    (plan, rows)
}

#[test]
fn folders_step_to_the_first_row_of_the_next_and_previous_folder() {
    let (plan, rows) = view();
    // root.txt, [a], one, Two, [b], three
    let header = |at: u32| matches!(rows[at as usize], ViewRow::Folder { .. });
    assert!(header(1) && header(4), "{rows:?}");
    assert_eq!(find_row(&plan, &rows, &RowQuery::NextFolder { from: 0 }), Some(2));
    assert_eq!(find_row(&plan, &rows, &RowQuery::NextFolder { from: 2 }), Some(5));
    assert_eq!(find_row(&plan, &rows, &RowQuery::NextFolder { from: 5 }), None);
    assert_eq!(find_row(&plan, &rows, &RowQuery::PreviousFolder { from: 5 }), Some(2));
    assert_eq!(
        find_row(&plan, &rows, &RowQuery::PreviousFolder { from: 3 }),
        Some(0),
        "the root's rows"
    );
    assert_eq!(find_row(&plan, &rows, &RowQuery::PreviousFolder { from: 0 }), None);
}

#[test]
fn names_match_by_prefix_ignoring_case_and_wrap() {
    let (plan, rows) = view();
    let name = |from: u32, prefix: &str| {
        find_row(
            &plan,
            &rows,
            &RowQuery::Name {
                from,
                prefix: prefix.to_owned(),
            },
        )
    };
    let name_at = |at: Option<u32>| match at.map(|at| &rows[at as usize]) {
        Some(ViewRow::Item { index }) => plan.items[*index as usize]
            .left
            .as_ref()
            .map(|entry| entry.rel_path.clone()),
        _ => None,
    };
    assert_eq!(name_at(name(0, "tw")).as_deref(), Some("a/Two.txt"), "case is ignored");
    let three = name(0, "three").expect("three");
    assert_eq!(
        name_at(name(three + 1, "r")).as_deref(),
        Some("root.txt"),
        "wraps past the end"
    );
    assert_eq!(name(0, "zzz"), None);
    assert_eq!(name(0, ""), None);
}
