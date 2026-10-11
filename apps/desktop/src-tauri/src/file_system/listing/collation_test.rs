//! The order names themselves come out in (`collation.rs`).
//!
//! Separate from the column-ordering suite because it answers a different
//! question and needs a different fixture: these tests pin the process-wide
//! reading language, sharing a lock with `intl::native_strings`, since the OS
//! locale otherwise decides where `ö` sits and a Swedish machine disagrees with
//! a US runner.
//!
//! Which column and which direction is `sorting_test`; the directory-vs-file
//! policy is `sorting_dir_mode_test`.

use super::sorting::DirectorySortMode;
use super::sorting::entry_comparator;
use super::sorting::sort_entries;
use super::sorting_test_support::make_entry;
use super::{FileEntry, SortColumn, SortOrder};

// ============================================================================
// Unicode collation
// ============================================================================

/// Pins the language names collate in, for the test's whole lifetime.
///
/// `sort_entries` orders names in the language the user reads, which is
/// process-wide state: a sibling test in `intl::native_strings` writes it, and
/// left alone it resolves to the OS locale, where a Swedish machine would seat
/// `ö` after `z` and fail an assertion that a US runner passes. Both sides hold
/// the same lock; see `intl::native_strings::lock_active_locale_for_tests`.
struct PinnedLocale(
    #[allow(
        dead_code,
        reason = "holding the guard IS the effect; dropping it early re-opens the window it closes"
    )]
    std::sync::MutexGuard<'static, ()>,
);

impl PinnedLocale {
    fn to(tag: &str) -> Self {
        let guard = crate::intl::native_strings::lock_active_locale_for_tests();
        let _ = crate::intl::set_language_preference(Some(tag.to_string()));
        Self(guard)
    }
}

impl Drop for PinnedLocale {
    fn drop(&mut self) {
        // Back to following the OS, so nothing after this sees a pinned language.
        let _ = crate::intl::set_language_preference(None);
    }
}

/// macOS stores whatever bytes a writer hands it, so one folder routinely holds
/// both spellings of the same accented name: Cocoa-based writers produce NFD
/// (`a` + U+0301), browsers and most others NFC (U+00E1). Ordering by raw code
/// point compares `a` (0x61) against `á` (0xE1) and decides right there, which
/// put `…Dávid…-signed` above `…Dáviad…-araw` in a real `~/Downloads`.
#[test]
fn mixed_unicode_normalization_orders_by_the_visible_name() {
    let _locale = PinnedLocale::to("en");
    let nfd = "Veszelovszki_Da\u{301}vid_She\u{2019}s-signed.pdf";
    let nfc = "Veszelovszki_D\u{e1}viad_She\u{2019}s-araw.pdf";

    let mut entries = vec![
        make_entry(nfd, false, Some(100), None),
        make_entry(nfc, false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // "Dávi*a*d" precedes "Dávi*d*": the `a`/`d` at the fifth character decides,
    // and the normalization spelling of the `á` before it must not.
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec![nfc, nfd]);
}

/// The two spellings of one name are the same name, so they tie on collation
/// weight. The comparator still has to pick a stable, repeatable winner, or the
/// watcher's re-sort sees a phantom `DiffChangeType::Move`.
#[test]
fn the_two_spellings_of_one_name_are_ordered_deterministically() {
    let _locale = PinnedLocale::to("en");
    let nfd = "Da\u{301}vid.txt";
    let nfc = "D\u{e1}vid.txt";

    let sort_both = |first: &str, second: &str| {
        let mut entries = vec![
            make_entry(first, false, Some(100), None),
            make_entry(second, false, Some(100), None),
        ];
        sort_entries(
            &mut entries,
            SortColumn::Name,
            SortOrder::Ascending,
            DirectorySortMode::LikeFiles,
        );
        entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
    };

    // Same pair, opposite input order: the result must not follow the input.
    assert_eq!(sort_both(nfd, nfc), sort_both(nfc, nfd));
}

/// `_` is U+005F and the digits are U+0030-U+0039, so code-point order forces
/// every `_name` below every `2026-…` name. Unicode collation orders by category
/// instead (punctuation, then digits, then letters), which is what Finder, Total
/// Commander, and Explorer all show.
#[test]
fn underscore_sorts_before_digits() {
    let _locale = PinnedLocale::to("en");
    let mut entries = vec![
        make_entry("2026-08-27 David pixel pics", true, None, None),
        make_entry("_cameras", true, None, None),
        make_entry("2023", true, None, None),
        make_entry("_years", true, None, None),
        make_entry("calls", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["_cameras", "_years", "2023", "2026-08-27 David pixel pics", "calls",]
    );
}

/// Code-point order puts every accented letter after `z`. Collation seats each
/// one next to the base letter it belongs to.
#[test]
fn accented_letters_sort_beside_their_base_letter() {
    let _locale = PinnedLocale::to("en");
    let mut entries = vec![
        make_entry("zebra.txt", false, Some(1), None),
        make_entry("\u{e1}rv\u{ed}zt\u{171}r\u{151}.txt", false, Some(1), None),
        make_entry("alma.txt", false, Some(1), None),
        make_entry("bor.txt", false, Some(1), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "alma.txt",
            "\u{e1}rv\u{ed}zt\u{171}r\u{151}.txt",
            "bor.txt",
            "zebra.txt",
        ]
    );
}

/// The bulk sort builds one collation key per row; the incremental insert path
/// compares names live. They are two readings of one order, so a name pair must
/// never rank differently depending on which one asked.
#[test]
fn the_bulk_sort_and_the_live_comparator_agree_on_unicode_names() {
    let _locale = PinnedLocale::to("en");
    let names = [
        "_cameras",
        "2023",
        "2026-08-27 David pixel pics",
        "Da\u{301}vid.txt",
        "D\u{e1}viad.txt",
        "\u{e1}rv\u{ed}zt\u{171}r\u{151}.txt",
        "alma.txt",
        "img_2.png",
        "img_10.png",
        "\u{f6}ver.txt",
        "zebra.txt",
    ];
    let entries: Vec<FileEntry> = names.iter().map(|n| make_entry(n, false, Some(1), None)).collect();

    let mut via_sort = entries.clone();
    sort_entries(
        &mut via_sort,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let mut via_cmp = entries.clone();
    via_cmp.sort_by(entry_comparator(
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    ));

    let sorted: Vec<&str> = via_sort.iter().map(|e| e.name.as_str()).collect();
    let compared: Vec<&str> = via_cmp.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(sorted, compared);
}

/// Case is a collation question, not a byte one: `to_lowercase` would rank
/// `BETA` against `alpha` by folded bytes, where the collator seats them the way
/// a reader expects whatever case each name happens to carry.
#[test]
fn test_case_insensitive_sort() {
    let _locale = PinnedLocale::to("en");
    let mut entries = vec![
        make_entry("Zebra.txt", false, Some(100), None),
        make_entry("alpha.txt", false, Some(100), None),
        make_entry("BETA.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha.txt", "BETA.txt", "Zebra.txt"]);
}

// ============================================================================
// Golden order, per locale
// ============================================================================

/// One corpus that touches every rule a name sort has: NFC and NFD spellings
/// of one name, numerics, a `_` prefix, case pairs, and letters a locale seats
/// differently (`å`, `ö` sit after `z` in Swedish, beside `a` / `o` in English).
fn golden_corpus() -> Vec<FileEntry> {
    [
        "file10.txt",
        "file2.txt",
        "file1.txt",
        "_build",
        "Apple",
        "apple",
        "Zebra",
        "zebra",
        "\u{c5}ngstr\u{f6}m",
        "A\u{30a}ngstro\u{308}m",
        "\u{f6}l",
        "ol",
        "oz",
        "10.txt",
        "2.txt",
        "\u{c9}mile",
        "emile",
        "zoo",
    ]
    .into_iter()
    .map(|name| make_entry(name, false, None, None))
    .collect()
}

fn golden_order(locale: &str) -> Vec<String> {
    let _locale = PinnedLocale::to(locale);
    let mut entries = golden_corpus();
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    entries.into_iter().map(|e| e.name).collect()
}

/// Pinned so moving the collator from the process-wide language to an injected
/// locale (issue #301) provably orders every name the same way.
#[test]
fn the_golden_corpus_orders_the_same_in_english() {
    assert_eq!(
        golden_order("en"),
        [
            "_build",
            "2.txt",
            "10.txt",
            "A\u{30a}ngstro\u{308}m",
            "\u{c5}ngstr\u{f6}m",
            "apple",
            "Apple",
            "emile",
            "\u{c9}mile",
            "file1.txt",
            "file2.txt",
            "file10.txt",
            "ol",
            "\u{f6}l",
            "oz",
            "zebra",
            "Zebra",
            "zoo",
        ]
    );
}

#[test]
fn the_golden_corpus_orders_the_same_in_swedish() {
    assert_eq!(
        golden_order("sv"),
        [
            "_build",
            "2.txt",
            "10.txt",
            "apple",
            "Apple",
            "emile",
            "\u{c9}mile",
            "file1.txt",
            "file2.txt",
            "file10.txt",
            "ol",
            "oz",
            "zebra",
            "Zebra",
            "zoo",
            "A\u{30a}ngstro\u{308}m",
            "\u{c5}ngstr\u{f6}m",
            "\u{f6}l",
        ]
    );
}
