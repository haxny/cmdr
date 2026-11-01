use crate::filter::*;

fn filter(masks: &str) -> MaskFilter {
    MaskFilter::parse(masks, &[]).unwrap()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_splits_includes_and_excludes() {
    let f = filter("*.jpg *.png | raw/");
    assert_eq!(f.admits("a.jpg", false), Admission::In);
    assert_eq!(f.admits("dir/b.png", false), Admission::In);
    assert_eq!(f.admits("a.txt", false), Admission::Out);
    assert_eq!(f.admits("raw/x.jpg", false), Admission::Out);
    assert_eq!(f.admits("raw", true), Admission::Out);
}

#[test]
fn semicolons_separate_masks_too() {
    let f = filter("*.jpg;*.png");
    assert_eq!(f.admits("a.png", false), Admission::In);
    assert_eq!(f.admits("a.gif", false), Admission::Out);
}

#[test]
fn empty_include_part_admits_everything() {
    let f = filter("| *.tmp");
    assert_eq!(f.admits("a.txt", false), Admission::In);
    assert_eq!(f.admits("a.tmp", false), Admission::Out);
    assert_eq!(filter("").admits("anything", false), Admission::In);
}

#[test]
fn folders_are_walked_whatever_the_includes_say() {
    let f = filter("*.jpg");
    assert_eq!(f.admits("photos", true), Admission::In);
    assert_eq!(f.admits("photos/a.jpg", false), Admission::In);
}

#[test]
fn trailing_slash_matches_folders_only() {
    let f = filter("| build/");
    assert_eq!(f.admits("build", true), Admission::Out);
    assert_eq!(f.admits("build", false), Admission::In);
    assert_eq!(f.admits("a/build", true), Admission::Out);
}

#[test]
fn leading_slash_anchors_at_the_root() {
    let f = filter("|/cache/");
    assert_eq!(f.admits("cache", true), Admission::Out);
    assert_eq!(f.admits("a/cache", true), Admission::In);
    assert_eq!(f.admits("cache/x.bin", false), Admission::Out);
}

#[test]
fn question_mark_matches_one_character() {
    let f = filter("| a?c.txt");
    assert_eq!(f.admits("abc.txt", false), Admission::Out);
    assert_eq!(f.admits("ac.txt", false), Admission::In);
    assert_eq!(f.admits("abbc.txt", false), Admission::In);
}

#[test]
fn cmdr_sync_folder_is_always_out() {
    let f = filter("");
    assert_eq!(f.admits(".cmdr-sync", true), Admission::Out);
    assert_eq!(f.admits("a/.cmdr-sync", true), Admission::Out);
    assert_eq!(f.admits("a/.cmdr-sync/keep.bin", false), Admission::Out);
    assert_eq!(f.admits(".CMDR-SYNC", true), Admission::Out);
}

#[test]
fn junk_answers_junk_and_other_excludes_out() {
    let defaults = strings(DEFAULT_EXCLUDES);
    let f = MaskFilter::parse("", &defaults).unwrap();
    assert_eq!(f.admits(".DS_Store", false), Admission::Junk);
    assert_eq!(f.admits("a/._photo.jpg", false), Admission::Junk);
    assert_eq!(f.admits("Thumbs.db", false), Admission::Junk);
    assert_eq!(f.admits(".Trashes", true), Admission::Out);
    assert_eq!(f.admits(".Spotlight-V100", true), Admission::Out);
    assert_eq!(f.admits("notes.txt", false), Admission::In);
}

#[test]
fn a_junk_mask_typed_in_the_masks_box_is_junk_too() {
    let f = filter("| .DS_Store");
    assert_eq!(f.admits("x/.DS_Store", false), Admission::Junk);
}

#[test]
fn masks_fold_case_and_unicode_form() {
    let f = filter("*.JPG | Caf\u{e9}/");
    assert_eq!(f.admits("a.jpg", false), Admission::In);
    // Decomposed spelling of the excluded folder.
    assert_eq!(f.admits("cafe\u{301}", true), Admission::Out);
    assert_eq!(f.admits("CAF\u{c9}", true), Admission::Out);
}

#[test]
fn quoted_mask_keeps_its_spaces() {
    let f = filter("| \"my notes*\"");
    assert_eq!(f.admits("my notes.txt", false), Admission::Out);
    assert_eq!(f.admits("my.txt", false), Admission::In);
    assert_eq!(f.admits("notes.txt", false), Admission::In);
}

#[test]
fn editable_excludes_behave_like_masks_after_the_bar() {
    let f = MaskFilter::parse("*.txt", &strings(&["draft*"])).unwrap();
    assert_eq!(f.admits("draft1.txt", false), Admission::Out);
    assert_eq!(f.admits("final.txt", false), Admission::In);
}

#[test]
fn empty_mask_after_bar_is_refused() {
    assert!(matches!(
        MaskFilter::parse("*.jpg | ", &[]),
        Err(MaskError::EmptyMask { .. })
    ));
    assert!(matches!(
        MaskFilter::parse("*.jpg", &strings(&[" "])),
        Err(MaskError::EmptyMask { .. })
    ));
}

#[test]
fn unclosed_quote_and_second_bar_are_refused() {
    assert!(matches!(
        MaskFilter::parse("\"abc", &[]),
        Err(MaskError::UnclosedQuote { position: 0 })
    ));
    assert!(matches!(
        MaskFilter::parse("*.a | b | c", &[]),
        Err(MaskError::SecondBar { position: 8 })
    ));
}

#[test]
fn backslashes_from_windows_masks_work() {
    let f = filter("| temp\\");
    assert_eq!(f.admits("temp", true), Admission::Out);
    assert_eq!(f.admits("a/temp", true), Admission::Out);
    let g = filter("| \\sub\\*.ext");
    assert_eq!(g.admits("sub/a.ext", false), Admission::Out);
    assert_eq!(g.admits("x/sub/a.ext", false), Admission::In);
}
