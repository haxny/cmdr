use super::*;
use crate::log_private::CLOSER;

#[test]
fn short_text_prints_quoted_escaped_and_trimmed() {
    let text = "  tree connect failed: \"Private Share\"\nsecond line\n";
    assert_eq!(
        format!("detail={:?}", LogDetail(text)),
        r#"detail="tree connect failed: \"Private Share\"\nsecond line""#
    );
}

#[test]
fn long_text_is_cut_on_a_char_boundary_with_its_full_length() {
    let text = format!("{}é{}", "a".repeat(LOCAL_DETAIL_MAX_BYTES - 1), "b".repeat(100));
    let printed = format!("{:?}", LogDetail(&text));
    assert!(printed.starts_with(&format!("\"{}", "a".repeat(LOCAL_DETAIL_MAX_BYTES - 1))));
    assert!(printed.ends_with(&format!("…[{} bytes]\"", text.len())), "{printed}");
    assert!(!printed.contains("bb"), "{printed}");
}

/// External text that echoes a hostile name raw would otherwise quote it into a real depth-1
/// closer, the depth `LogDetail`'s own spans sit at.
#[test]
fn a_marker_char_in_external_text_prints_as_a_replacement_char() {
    let text = format!("open failed: /Volumes/Backup/evil{CLOSER}rest/child.pdf");
    let printed = format!("{:?}", LogDetail(&text));
    assert_eq!(printed, "\"open failed: /Volumes/Backup/evil\u{FFFD}rest/child.pdf\"");
    assert!(markers(&printed).is_empty(), "{printed}");
}

const FILE: &str = "/Users/me/Pictures/Summer (Lake House) 2026/IMG 0001.jpg";
const DIR: &str = "/Users/me/Pictures/Summer (Lake House) 2026";

/// What `marking` must print: the text with each occurrence swapped for its wrapper, Debug-quoted.
fn expected(text_with_wrappers: &str) -> String {
    format!("{text_with_wrappers:?}")
}

#[test]
fn marking_wraps_every_occurrence_of_a_value_one_depth_down() {
    let text = format!("open({FILE}): Permission denied; retried {FILE}");
    let printed = format!("{:?}", LogDetail(&text).marking(&[LogPrivate::file_path(FILE)]));
    let file = LogPrivate::file_path(FILE);
    assert_eq!(
        printed,
        expected(&format!("open({file}): Permission denied; retried {file}"))
    );
    let found = markers(&printed);
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(found.iter().all(|m| m.depth == 1), "{found:?}");
}

#[test]
fn the_longest_value_claims_first() {
    let text = format!("couldn't stat {FILE} while listing {DIR}");
    let printed = format!(
        "{:?}",
        LogDetail(&text).marking(&[LogPrivate::dir_path(DIR), LogPrivate::file_path(FILE)])
    );
    assert_eq!(
        printed,
        expected(&format!(
            "couldn't stat {} while listing {}",
            LogPrivate::file_path(FILE),
            LogPrivate::dir_path(DIR)
        ))
    );
}

#[test]
fn an_occurrence_glued_to_a_word_stays_text() {
    let printed = format!(
        "{:?}",
        LogDetail("PhotosBackup is not Photos").marking(&[LogPrivate::dir_name("Photos")])
    );
    assert_eq!(
        printed,
        expected(&format!("PhotosBackup is not {}", LogPrivate::dir_name("Photos")))
    );
}

/// A hostile name finds its own echo: both sides have their reserved chars replaced to compare,
/// and the span carries the RAW value escaped, so it hashes like the leaf wrapper's.
#[test]
fn a_hostile_name_is_marked_in_its_own_echo() {
    let hostile = format!("Summer{CLOSER}Lake \"House\"");
    let text = format!("tree connect failed for {hostile}: STATUS_BAD_NETWORK_NAME");
    let printed = format!(
        "{:?}",
        LogDetail(&text).marking(&[LogPrivate::volume(hostile.as_str())])
    );
    assert_eq!(
        printed,
        expected(&format!(
            "tree connect failed for {}: STATUS_BAD_NETWORK_NAME",
            LogPrivate::volume(hostile.as_str())
        ))
    );
    let own: Vec<_> = markers(&printed).into_iter().filter(|m| m.depth == 1).collect();
    assert_eq!(own.len(), 2, "{printed}");
}

#[test]
fn no_values_or_empty_values_mark_nothing() {
    let text = "plain stderr";
    for values in [&[][..], &[LogPrivate::name("")][..]] {
        assert_eq!(format!("{:?}", LogDetail(text).marking(values)), "\"plain stderr\"");
    }
}

/// Every depth-1 opener in `printed` has its closer: the span-aware cap never splits one.
fn assert_spans_balanced(printed: &str) {
    let mut open = None;
    for marker in markers(printed) {
        match (open, marker.role) {
            (None, MarkerRole::Open(_)) => open = Some(marker.depth),
            (Some(depth), MarkerRole::Close) if depth == marker.depth => open = None,
            (Some(_), _) => {}
            (None, MarkerRole::Close) => panic!("stray closer: {printed}"),
        }
    }
    assert!(open.is_none(), "an open span: {printed}");
}

#[test]
fn the_cap_never_splits_a_marked_value_at_any_offset() {
    let value = "Summer (Lake House) \"2026\"\\x";
    for lead in LOCAL_DETAIL_MAX_BYTES - value.len() - 2..=LOCAL_DETAIL_MAX_BYTES + 2 {
        let text = format!("{}{value} tail", "-".repeat(lead));
        let printed = format!("{:?}", LogDetail(&text).marking(&[LogPrivate::dir_name(value)]));
        assert_spans_balanced(&printed);
        assert!(printed.ends_with(&format!("…[{} bytes]\"", text.len())), "{printed}");
        let kept = markers(&printed).len();
        assert!(kept == 0 || kept == 2, "lead {lead}: {printed}");
        // Never more than the cap of the original text, so a value that fits whole keeps its span.
        if lead + value.len() <= LOCAL_DETAIL_MAX_BYTES {
            assert_eq!(kept, 2, "lead {lead}: {printed}");
        }
    }
}

#[test]
fn logged_text_keeps_its_markers_one_depth_down() {
    let message = format!(
        "listing {} failed: detail={:?}",
        LogPrivate::file_path(FILE),
        LogDetail(&format!("opendir {DIR}: EACCES")).marking(&[LogPrivate::dir_path(DIR)])
    );
    let printed = format!("{:?}", LoggedText(&message));
    assert_eq!(printed, format!("{message:?}"));
    let depths: Vec<u32> = markers(&printed).iter().map(|m| m.depth).collect();
    assert_eq!(depths, [1, 1, 2, 2], "{printed}");
}

/// A raw stray closer from an unmarked site isn't ours to fix: it stays a stray, one depth
/// down, for the report pass to fail closed on.
#[test]
fn logged_text_keeps_a_stray_marker_as_a_stray() {
    let message = format!("couldn't open /Volumes/Backup/evil{CLOSER}rest/child.pdf");
    let printed = format!("{:?}", LoggedText(&message));
    assert_eq!(printed, format!("{message:?}"));
    assert_eq!(markers(&printed).len(), 1);
}

#[test]
fn logged_text_cap_never_splits_a_span_at_any_offset_or_depth() {
    let value = "Summer (Lake House) \"2026\"\\x";
    let leaf = LogPrivate::file_name(value);
    let spellings = [
        format!("{leaf}"),
        format!("name={leaf:?}"),
        format!(
            "detail={:?}",
            LogDetail(&format!("open {value} failed")).marking(&[leaf])
        ),
    ];
    for spelling in &spellings {
        for lead in LOCAL_DETAIL_MAX_BYTES.saturating_sub(spelling.len() + 2)..=LOCAL_DETAIL_MAX_BYTES + 2 {
            let message = format!("{}{spelling} tail", "a".repeat(lead));
            let printed = format!("{:?}", LoggedText(&message));
            assert_spans_balanced(&printed);
            if lead + spelling.len() <= LOCAL_DETAIL_MAX_BYTES {
                assert!(!markers(&printed).is_empty(), "lead {lead}: {printed}");
            }
        }
    }
}

#[test]
fn logged_text_short_text_is_its_debug_quoting() {
    assert_eq!(format!("{:?}", LoggedText("  a \"b\"\n")), "\"a \\\"b\\\"\"");
}
