//! The case-only rename guard: which destination a casefolded `/sdcard` takes
//! for the source itself.

use super::is_the_source_recased;

#[test]
fn an_ascii_recasing_is_the_source() {
    assert!(is_the_source_recased("/sdcard/Album", "/sdcard/album"));
}

/// ❗ **Case folding is Unicode's, not ASCII's** (#512): Android's casefolded
/// storage folds `Ö` and `ö` (and `ß` and `SS`) together, so an ASCII-only
/// guard read a re-cased folder as another one and `rmdir`ed the source (or
/// refused with `ENOTEMPTY`).
#[test]
fn a_non_ascii_recasing_is_the_source() {
    assert!(is_the_source_recased("/sdcard/Ölmappe", "/sdcard/ölmappe"));
    assert!(is_the_source_recased("/sdcard/ΣΟΦΙΑΣ", "/sdcard/σοφιας"));
    assert!(is_the_source_recased("/sdcard/Straße", "/sdcard/STRASSE"));
}

/// The same name in either Unicode form, re-cased, is the source too.
#[test]
fn a_recasing_across_unicode_forms_is_the_source() {
    assert!(is_the_source_recased("/sdcard/Caf\u{e9}", "/sdcard/CAFE\u{301}"));
}

#[test]
fn another_name_is_not_the_source() {
    assert!(!is_the_source_recased("/sdcard/Album", "/sdcard/Albums"));
    assert!(!is_the_source_recased("/sdcard/Ölmappe", "/sdcard/Olmappe"));
}
