use std::path::Path;

use crate::test_tree::{Tree, key};
use crate::*;

const T: u64 = 1_700_000_000;

fn options(mode: Mode) -> PlanOptions {
    PlanOptions {
        mode,
        rules: CompareRules::QUICK,
    }
}

fn plan_of(mode: Mode, left: Tree, right: Tree) -> Plan {
    plan(&left.build(), &right.build(), None, &options(mode))
}

fn item<'a>(plan: &'a Plan, path: &str) -> &'a PlanItem {
    let wanted = key(path);
    plan.items
        .iter()
        .find(|item| item.key == wanted)
        .unwrap_or_else(|| panic!("no item for {path}"))
}

fn has_item(plan: &Plan, path: &str) -> bool {
    let wanted = key(path);
    plan.items.iter().any(|item| item.key == wanted)
}

#[test]
fn default_actions_match_the_spec_table() {
    use Action::{CopyLeft, CopyRight, Equal, RemoveRight, Skip};
    // Columns: Compare only, Both ways, Update →, Mirror → (spec 6.4).
    let table: [(Category, [Action; 4]); 10] = [
        (Category::LeftOnly, [Skip, CopyRight, CopyRight, CopyRight]),
        (Category::RightOnly, [Skip, CopyLeft, Skip, RemoveRight]),
        (Category::NewerLeft, [Skip, CopyRight, CopyRight, CopyRight]),
        (Category::NewerRight, [Skip, CopyLeft, Skip, CopyRight]),
        (Category::Different, [Skip, Skip, Skip, CopyRight]),
        (Category::Unknown, [Skip, Skip, Skip, Skip]),
        (Category::Equal, [Equal, Equal, Equal, Equal]),
        (Category::TypeClash, [Skip, Skip, Skip, Skip]),
        (Category::NameClash, [Skip, Skip, Skip, Skip]),
        (Category::Unreadable, [Skip, Skip, Skip, Skip]),
    ];
    let modes = [Mode::CompareOnly, Mode::BothWays, Mode::Update, Mode::Mirror];
    assert_eq!(modes, Mode::ALL);
    for (category, row) in table {
        for (mode, want) in modes.into_iter().zip(row) {
            assert_eq!(default_action(mode, category), want, "{mode:?} × {category:?}");
        }
    }
}

#[test]
fn items_are_sorted_by_key_and_indexed_by_position() {
    let left = Tree::new()
        .file("b.txt", 1, T)
        .file("a/x.txt", 1, T)
        .file("c.txt", 1, T);
    let plan = plan_of(Mode::Mirror, left, Tree::new().file("a/y.txt", 2, T));
    let keys: Vec<_> = plan.items.iter().map(|item| item.key.clone()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    for (position, item) in plan.items.iter().enumerate() {
        assert_eq!(item.index as usize, position);
    }
}

#[test]
fn mirror_marks_older_overwrites_newer() {
    let left = Tree::new().file("a.txt", 10, T).file("b.txt", 10, T + 500);
    let right = Tree::new().file("a.txt", 12, T + 100).file("b.txt", 12, T);
    let mirror = plan_of(Mode::Mirror, left.clone(), right.clone());
    let a = item(&mirror, "a.txt");
    assert_eq!(a.verdict.category, Category::NewerRight);
    assert_eq!(a.action, Action::CopyRight);
    assert!(a.older_overwrites_newer);
    let b = item(&mirror, "b.txt");
    assert_eq!(b.action, Action::CopyRight);
    assert!(!b.older_overwrites_newer);

    let both = plan_of(Mode::BothWays, left, right);
    assert_eq!(item(&both, "a.txt").action, Action::CopyLeft);
    assert!(!item(&both, "a.txt").older_overwrites_newer);
}

#[test]
fn clash_rows_never_get_a_default_action() {
    // `Report.pdf` + `report.pdf` on a case-sensitive left, one file on the right.
    let left = Tree::new().clash("report.pdf").file("docs/a.txt", 3, T);
    let right = Tree::new().file("report.pdf", 5, T).clash("docs");
    for mode in Mode::ALL {
        let plan = plan_of(mode, left.clone(), right.clone());
        let report = item(&plan, "report.pdf");
        assert_eq!(report.verdict.category, Category::NameClash, "{mode:?}");
        assert_eq!(report.default_action, Action::Skip);
        assert_eq!(report.action, Action::Skip);
        let docs = item(&plan, "docs");
        assert_eq!(docs.verdict.category, Category::NameClash);
        assert_eq!(docs.action, Action::Skip);
        assert!(
            !has_item(&plan, "docs/a.txt"),
            "the clash row covers what lies under it"
        );
    }
}

#[test]
fn unreadable_subtree_rows_have_no_action() {
    let left = Tree::new().unreadable("photos").file("keep.txt", 1, T);
    let right = Tree::new()
        .file("photos/a.jpg", 5, T)
        .file("photos/sub/b.jpg", 5, T)
        .file("keep.txt", 1, T);
    let plan = plan_of(Mode::Mirror, left, right);
    let photos = item(&plan, "photos");
    assert_eq!(photos.verdict.category, Category::Unreadable);
    assert_eq!(photos.action, Action::Skip);
    assert!(!has_item(&plan, "photos/a.jpg"));
    assert!(plan.items.iter().all(|item| item.action != Action::RemoveRight));

    // Unreadable on the target: nothing is copied into what we can't see.
    let left = Tree::new().file("photos/new.jpg", 5, T);
    let right = Tree::new().unreadable("photos");
    let plan = plan_of(Mode::Mirror, left, right);
    for item in &plan.items {
        assert_eq!(item.action, Action::Skip, "{:?}", item.key);
    }
}

#[test]
fn one_sided_folder_is_one_item_unless_it_holds_back_entries() {
    let left = Tree::new()
        .file("music/a.mp3", 10, T)
        .file("music/b/c.mp3", 20, T)
        .file("docs/a.txt", 4, T)
        .held_back("docs", 1)
        .file("pics/x.jpg", 6, T)
        .unreadable("pics/locked")
        .file("mail/m.eml", 2, T)
        .clash("mail/inbox");
    let plan = plan_of(Mode::Mirror, left, Tree::new().file("other.txt", 1, T));

    let music = item(&plan, "music");
    assert_eq!(music.action, Action::CopyRight);
    assert_eq!(music.bytes, 30);
    assert!(!has_item(&plan, "music/a.mp3"));
    assert!(!has_item(&plan, "music/b"));

    assert!(!has_item(&plan, "docs"), "held back entries split the folder");
    assert_eq!(item(&plan, "docs/a.txt").action, Action::CopyRight);

    assert!(!has_item(&plan, "pics"), "an unreadable folder inside splits it");
    assert_eq!(item(&plan, "pics/x.jpg").action, Action::CopyRight);
    assert_eq!(item(&plan, "pics/locked").verdict.category, Category::Unreadable);
    assert_eq!(item(&plan, "pics/locked").action, Action::Skip);

    assert!(!has_item(&plan, "mail"), "a clash inside splits it");
    assert_eq!(item(&plan, "mail/m.eml").action, Action::CopyRight);
    assert_eq!(item(&plan, "mail/inbox").action, Action::Skip);
}

#[test]
fn right_only_folder_holding_back_entries_is_never_removed_whole() {
    let right = Tree::new().file("old/a.txt", 1, T).held_back("old", 2);
    let plan = plan_of(Mode::Mirror, Tree::new().file("x", 1, T), right);
    assert!(!has_item(&plan, "old"));
    assert_eq!(item(&plan, "old/a.txt").action, Action::RemoveRight);
}

#[test]
fn both_present_folders_head_their_children_and_type_clashes_cover_theirs() {
    let left = Tree::new().file("shared/a.txt", 1, T).file("x", 3, T);
    let right = Tree::new().file("shared/a.txt", 1, T).file("x/inner.txt", 1, T);
    let plan = plan_of(Mode::Mirror, left, right);
    assert!(!has_item(&plan, "shared"));
    assert_eq!(item(&plan, "shared/a.txt").action, Action::Equal);
    assert_eq!(item(&plan, "x").verdict.category, Category::TypeClash);
    assert_eq!(item(&plan, "x").action, Action::Skip);
    assert!(!has_item(&plan, "x/inner.txt"));
}

#[test]
fn defaults_never_write_to_a_read_only_side() {
    let left = Tree::new().file("a.txt", 1, T);
    let right = Tree::new().file("b.txt", 1, T).read_only();
    let plan = plan_of(Mode::Mirror, left, right);
    assert_eq!(item(&plan, "a.txt").default_action, Action::Skip);
    assert_eq!(item(&plan, "b.txt").default_action, Action::Skip);
}

#[test]
fn content_mode_asks_for_equal_size_pairs_only() {
    let left = Tree::new()
        .file("same.txt", 5, T)
        .file("big.txt", 9, T)
        .file("solo.txt", 1, T);
    let right = Tree::new().file("same.txt", 5, T + 100).file("big.txt", 4, T);
    let options = PlanOptions {
        mode: Mode::BothWays,
        rules: CompareRules {
            by: CompareBy::Content,
            ..CompareRules::QUICK
        },
    };
    let plan = plan(&left.build(), &right.build(), None, &options);
    let same = item(&plan, "same.txt");
    assert_eq!(same.content, ContentState::Pending);
    assert_eq!(same.verdict.category, Category::Unknown);
    assert_eq!(same.action, Action::Skip);
    assert_eq!(item(&plan, "big.txt").content, ContentState::NotAsked);

    let jobs = content_jobs(&plan, Path::new("/L"), Path::new("/R"));
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].item, same.index);
    assert_eq!(jobs[0].left, Path::new("/L/same.txt"));
    assert_eq!(jobs[0].right, Path::new("/R/same.txt"));
    assert_eq!(jobs[0].size, 5);
}

#[test]
fn reclassify_keeps_actions_changed_by_you() {
    let left = Tree::new()
        .file("a.txt", 5, T + 100)
        .file("b.txt", 5, T + 100)
        .file("c.txt", 5, T);
    let right = Tree::new().file("a.txt", 5, T).file("b.txt", 5, T).file("c.txt", 5, T);
    let options = PlanOptions {
        mode: Mode::BothWays,
        rules: CompareRules {
            by: CompareBy::Content,
            ..CompareRules::QUICK
        },
    };
    let caps = test_tree::caps();
    let planned = plan(&left.build(), &right.build(), None, &options);
    let a = item(&planned, "a.txt").index;
    let b = item(&planned, "b.txt").index;
    let c = item(&planned, "c.txt").index;
    let changed = apply_overrides(
        &planned,
        &[ActionChange {
            item: a,
            action: Action::CopyLeft,
        }],
        &caps,
        &caps,
    )
    .unwrap();

    let results = [
        (a, ContentState::Differs),
        (b, ContentState::Differs),
        (c, ContentState::Same),
    ];
    let after = reclassify(&changed, &results);
    let (a, b, c) = (
        &after.items[a as usize],
        &after.items[b as usize],
        &after.items[c as usize],
    );
    assert_eq!(a.content, ContentState::Differs);
    assert_eq!(a.verdict.category, Category::NewerLeft);
    assert_eq!(a.default_action, Action::CopyRight);
    assert_eq!(a.action, Action::CopyLeft, "changed by you, kept");
    assert!(a.older_overwrites_newer);
    assert_eq!(b.action, Action::CopyRight, "follows its new default");
    assert_eq!(c.verdict.category, Category::Equal);
    assert_eq!(c.action, Action::Equal);
    assert!(content_jobs(&after, Path::new("/L"), Path::new("/R")).is_empty());
}
