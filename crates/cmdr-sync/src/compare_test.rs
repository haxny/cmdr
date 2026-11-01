use crate::*;

fn file(size: u64, mtime: u64) -> EntryFacts {
    EntryFacts {
        kind: EntryKind::File,
        size: Some(size),
        mtime: Some(mtime),
    }
}

fn folder() -> EntryFacts {
    EntryFacts {
        kind: EntryKind::Folder,
        size: None,
        mtime: Some(1),
    }
}

fn run(l: &EntryFacts, r: &EntryFacts, rules: &CompareRules) -> Verdict {
    classify(Some(l), Some(r), rules, ContentState::NotAsked)
}

#[test]
fn tolerance_edges_one_two_three_seconds() {
    let r = CompareRules::QUICK;
    assert_eq!(run(&file(5, 101), &file(5, 100), &r).category, Category::Equal);
    assert_eq!(run(&file(5, 102), &file(5, 100), &r).category, Category::Equal);
    let v = run(&file(5, 103), &file(5, 100), &r);
    assert_eq!((v.category, v.same_size), (Category::NewerLeft, true));
    assert_eq!(run(&file(5, 100), &file(5, 103), &r).category, Category::NewerRight);
}

#[test]
fn exact_hour_shift_is_equal_only_when_enabled() {
    let on = CompareRules {
        ignore_hour_shift: true,
        tolerance_secs: 0,
        ..CompareRules::QUICK
    };
    let off = CompareRules {
        tolerance_secs: 0,
        ..CompareRules::QUICK
    };
    assert_eq!(run(&file(5, 3700), &file(5, 100), &on).category, Category::Equal);
    assert_eq!(run(&file(5, 100), &file(5, 3700), &on).category, Category::Equal);
    assert_eq!(run(&file(5, 3700), &file(5, 100), &off).category, Category::NewerLeft);
    assert_eq!(run(&file(5, 3699), &file(5, 100), &on).category, Category::NewerLeft);
    assert_eq!(run(&file(5, 3701), &file(5, 100), &on).category, Category::NewerLeft);
}

#[test]
fn missing_mtime_never_claims_newer() {
    let none = EntryFacts {
        mtime: None,
        ..file(5, 0)
    };
    let r = CompareRules::QUICK;
    let v = run(&none, &file(7, 100), &r);
    assert_eq!((v.category, v.rung), (Category::Different, Rung::SizeOnly));
    assert_eq!(run(&none, &file(5, 100), &r).category, Category::Equal);
    assert_eq!(run(&file(5, 100), &none, &r).category, Category::Equal);
}

#[test]
fn right_offset_is_applied() {
    let r = CompareRules {
        right_offset_secs: 3600,
        ..CompareRules::QUICK
    };
    assert_eq!(run(&file(5, 7200), &file(5, 3600), &r).category, Category::Equal);
    assert_eq!(run(&file(5, 7200), &file(5, 0), &r).category, Category::NewerLeft);
}

#[test]
fn sizes_differ_dates_equal_is_different() {
    let v = run(&file(5, 100), &file(6, 100), &CompareRules::QUICK);
    assert_eq!(
        (v.category, v.rung, v.same_size),
        (Category::Different, Rung::SizeAndTime, false)
    );
}

#[test]
fn newer_wins_even_when_sizes_differ() {
    let v = run(&file(5, 200), &file(6, 100), &CompareRules::QUICK);
    assert_eq!((v.category, v.same_size), (Category::NewerLeft, false));
}

#[test]
fn size_only_ignores_dates_and_has_no_direction() {
    let r = CompareRules {
        by: CompareBy::SizeOnly,
        ..CompareRules::QUICK
    };
    assert_eq!(run(&file(5, 900), &file(5, 100), &r).category, Category::Equal);
    assert_eq!(run(&file(6, 900), &file(5, 100), &r).category, Category::Different);
    let unknown = EntryFacts {
        size: None,
        ..file(5, 100)
    };
    assert_eq!(run(&unknown, &file(5, 100), &r).category, Category::Unknown);
}

#[test]
fn ignore_date_decides_by_size() {
    let r = CompareRules {
        ignore_date: true,
        ..CompareRules::QUICK
    };
    assert_eq!(run(&file(5, 900), &file(5, 100), &r).category, Category::Equal);
    assert_eq!(run(&file(6, 900), &file(5, 100), &r).category, Category::Different);
}

#[test]
fn content_mode_takes_the_content_answer() {
    let r = CompareRules {
        by: CompareBy::Content,
        ..CompareRules::QUICK
    };
    let ask = |l: &EntryFacts, r_: &EntryFacts, c| classify(Some(l), Some(r_), &r, c);
    let (a, b) = (file(5, 200), file(5, 100));
    let same = ask(&a, &b, ContentState::Same);
    assert_eq!((same.category, same.rung), (Category::Equal, Rung::Content));
    assert_eq!(ask(&a, &b, ContentState::Differs).category, Category::NewerLeft);
    assert_eq!(
        ask(&file(5, 100), &file(5, 101), ContentState::Differs).category,
        Category::Different
    );
    assert_eq!(ask(&a, &b, ContentState::Pending).category, Category::Unknown);
    assert_eq!(ask(&a, &b, ContentState::NotAsked).category, Category::Unknown);
    assert_eq!(ask(&a, &b, ContentState::Unreadable).category, Category::Unknown);
    // Unequal sizes never need the bytes.
    assert_eq!(
        ask(&file(6, 100), &file(5, 100), ContentState::NotAsked).category,
        Category::Different
    );
}

#[test]
fn content_differs_with_ignore_date_has_no_direction() {
    let r = CompareRules {
        by: CompareBy::Content,
        ignore_date: true,
        ..CompareRules::QUICK
    };
    let v = classify(Some(&file(5, 900)), Some(&file(5, 100)), &r, ContentState::Differs);
    assert_eq!(v.category, Category::Different);
}

#[test]
fn file_against_folder_is_a_type_clash() {
    let v = run(&file(5, 1), &folder(), &CompareRules::QUICK);
    assert_eq!(v.category, Category::TypeClash);
    let link = EntryFacts {
        kind: EntryKind::Symlink,
        ..file(5, 1)
    };
    assert_eq!(
        run(&link, &file(5, 1), &CompareRules::QUICK).category,
        Category::TypeClash
    );
}

#[test]
fn two_folders_are_equal_and_one_side_is_presence() {
    assert_eq!(
        run(&folder(), &folder(), &CompareRules::QUICK).category,
        Category::Equal
    );
    let q = CompareRules::QUICK;
    let left = classify(Some(&file(1, 1)), None, &q, ContentState::NotAsked);
    assert_eq!((left.category, left.rung), (Category::LeftOnly, Rung::Presence));
    let right = classify(None, Some(&file(1, 1)), &q, ContentState::NotAsked);
    assert_eq!(right.category, Category::RightOnly);
}

fn caps(precision: u64) -> SyncCaps {
    SyncCaps {
        case_sensitive: true,
        names_in_any_unicode_form: false,
        mtime_precision_secs: precision,
        can_write: true,
        has_trash: false,
        atomic_replace: false,
        can_read_content: false,
        renames_can_copy: false,
    }
}

#[test]
fn widened_for_takes_the_coarser_precision() {
    let rules = CompareRules {
        tolerance_secs: 0,
        ..CompareRules::QUICK
    };
    assert_eq!(rules.widened_for(&caps(1), &caps(2)).tolerance_secs, 2);
    assert_eq!(rules.widened_for(&caps(1), &caps(1)).tolerance_secs, 1);
    assert_eq!(CompareRules::QUICK.widened_for(&caps(1), &caps(1)).tolerance_secs, 2);
}

#[test]
fn facts_of_an_entry_follow_its_flags() {
    let mut e = cmdr_fs::entry::FileEntry::new("a".into(), "/a".into(), false, false);
    e.size = Some(3);
    e.modified_at = Some(9);
    assert_eq!(EntryFacts::of(&e), file(3, 9));
    e.is_symlink = true;
    assert_eq!(EntryFacts::of(&e).kind, EntryKind::Symlink);
    e.is_symlink = false;
    e.is_directory = true;
    assert_eq!(EntryFacts::of(&e).kind, EntryKind::Folder);
}
