//! Tree order: folders first, then files, each by the panes' collation.

use std::cmp::Ordering;

use super::TreeOrder;
use cmdr_listing::testing::collator_for;

fn english() -> TreeOrder {
    TreeOrder::with_collator(collator_for("en"))
}

#[test]
fn folders_come_first_then_files_each_by_collation() {
    let mut items = vec![
        ("b.txt", false),
        ("img_10", true),
        ("a.txt", false),
        ("Zebra", true),
        ("img_9", true),
        ("\u{e1}rv\u{ed}z.txt", false),
    ];

    english().sort(&mut items, |(name, is_folder)| (*is_folder, *name));

    let names: Vec<&str> = items.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        vec!["img_9", "img_10", "Zebra", "a.txt", "\u{e1}rv\u{ed}z.txt", "b.txt"],
        "digit runs rank by value, accents sit beside their letter, and case doesn't lead"
    );
}

#[test]
fn compare_agrees_with_sort() {
    let order = english();
    let names = [
        ("a", true),
        ("B", true),
        ("a.txt", false),
        ("A.txt", false),
        ("10", false),
        ("9", false),
    ];
    let mut sorted = names.to_vec();
    order.sort(&mut sorted, |(name, is_folder)| (*is_folder, *name));

    for pair in sorted.windows(2) {
        assert_eq!(
            order.compare(pair[0].1, pair[0].0, pair[1].1, pair[1].0),
            Ordering::Less,
            "{:?} before {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn two_spellings_of_one_name_never_tie() {
    let order = english();
    let nfc = "D\u{e1}vid";
    let nfd = "Da\u{301}vid";
    assert_ne!(order.compare(false, nfc, false, nfd), Ordering::Equal);

    let mut both = vec![nfd, nfc];
    order.sort(&mut both, |name| (false, *name));
    let mut again = vec![nfc, nfd];
    order.sort(&mut again, |name| (false, *name));
    assert_eq!(both, again, "the order is total, whichever came first");
}
