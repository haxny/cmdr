//! The producers' real output through the span pass: `LogPrivate` at a `{}` and a `{:?}` site,
//! `LogDetail::marking`, and the hostile names each must survive. Every cell feeds what
//! `cmdr_fs` writes, never a hand-built line, and checks it hashes like the hand-built spellings
//! `markers_tests.rs` pins.

use super::markers_tests::{HEAD, assert_ships_none, context, hashes, quoted, r, shape, span};
use cmdr_fs::log_detail::{LOCAL_DETAIL_MAX_BYTES, LogDetail};
use cmdr_fs::log_private::{CLOSER, LogKind, LogPrivate, RESERVED};

const FILE: &str = "/Users/me/Pictures/Summer (Lake House) 2026/IMG 0001.jpg";

/// Names that try every way out of a span. Each holds `Lake` so a leak is easy to spot.
fn hostile_values() -> Vec<String> {
    let mut values: Vec<String> = RESERVED.map(|c| format!("Lake{c}House")).collect();
    values.extend(
        [
            "Summer (Lake House) 2026",
            "Lake\\u{fdd1}House",
            "Lake\\\\u{fdd1}House",
            "Lake \"House\"",
            "Lake House\\",
            "Lake\nHouse\r\t",
            "Lake\u{202E}fdp.exe",
            "Lake Cafe\u{301}",
            "<dir:abc>Lake",
            "Lake: House, a; b key=v -> )",
        ]
        .map(str::to_string),
    );
    values
}

#[test]
fn a_wrapped_value_hashes_like_the_hand_built_span_at_both_sites() {
    for kind in LogKind::ALL {
        for value in hostile_values() {
            let wrapped = LogPrivate::new(kind, value.as_str());
            let written = format!("{HEAD}moved {wrapped} item={wrapped:?} in 3ms");
            let by_hand = format!(
                "{HEAD}moved {} item={} in 3ms",
                span(kind, &value),
                quoted(kind, &value)
            );
            assert_eq!(written, by_hand, "{kind:?} {value:?}");
            assert_eq!(
                context().redact_line(&written),
                context().redact_line(&by_hand),
                "{kind:?} {value:?}"
            );
            let words = ["Lake", "House", "Summer", "fdp", "Cafe"];
            assert_ships_none(&written, &words);
            let tokens = hashes(&context().redact_line(&written));
            let (first, second) = tokens.split_at(tokens.len() / 2);
            assert!(!first.is_empty() && first == second, "{kind:?} {value:?}: {tokens:?}");
        }
    }
}

#[test]
fn a_wrapped_path_keeps_its_shape_and_kind() {
    let line = format!(
        "{HEAD}list_directory_core: 0 entries at {} (exists, readable). Could be genuinely empty",
        LogPrivate::dir_path("/Users/me/Pictures/com.example.app")
    );
    assert_eq!(
        r(&line),
        format!(
            "{HEAD}list_directory_core: 0 entries at $HOME/Pictures/<dir> (exists, readable). Could be genuinely empty"
        )
    );
    let line = format!("{HEAD}read path={:?}", LogPrivate::file_path(FILE));
    assert_eq!(
        shape(&line),
        format!("{HEAD}read path=\"$HOME/Pictures/<dir>/<file>.jpg\"")
    );
}

/// External text that echoes the caller's value carries it one depth down; the leaf and the echo
/// on one line hash to the one token.
#[test]
fn a_marked_detail_hashes_like_the_leaf_on_its_line() {
    let stderr = format!("open({FILE}): Permission denied");
    let line = format!(
        "{HEAD}copy failed path={:?} detail={:?}",
        LogPrivate::file_path(FILE),
        LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)])
    );
    assert_ships_none(&line, &["Lake", "House", "IMG", "0001"]);
    let out = context().redact_line(&line).into_owned();
    let tokens = hashes(&out);
    assert!(tokens.len() >= 2, "{out}");
    let file_tokens: Vec<_> = out.match_indices("<file:").map(|(at, _)| &out[at..at + 19]).collect();
    assert_eq!(file_tokens.len(), 2, "{out}");
    assert_eq!(file_tokens[0], file_tokens[1], "{out}");
}

#[test]
fn the_detail_cap_never_ships_a_name_at_any_offset() {
    for lead in LOCAL_DETAIL_MAX_BYTES - FILE.len() - 4..=LOCAL_DETAIL_MAX_BYTES + 4 {
        let stderr = format!("{} {FILE} tail", "-".repeat(lead));
        let line = format!(
            "{HEAD}copy failed detail={:?}",
            LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)])
        );
        assert_ships_none(&line, &["Lake", "House", "IMG", "0001"]);
    }
}

/// Plain `LogDetail` replaces a raw marker in foreign text, so an OS message echoing a hostile
/// name can't forge a closer at the depth real `LogDetail` spans sit at. The fallback regex still
/// claims the path whole.
#[test]
fn a_hostile_name_echoed_in_plain_external_text_is_claimed_whole() {
    let stderr = format!("open /Volumes/Backup/Secret Dir/evil{CLOSER}rest/child.pdf: EACCES");
    let line = format!("{HEAD}copy failed detail={:?}", LogDetail(&stderr));
    assert_ships_none(&line, &["Secret", "evil", "rest", "child"]);
}
