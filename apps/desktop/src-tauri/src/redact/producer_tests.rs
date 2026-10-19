//! The producers' real output through the span pass: `LogPrivate` at a `{}` and a `{:?}` site,
//! `LogDetail::marking`, and the hostile names each must survive. Every cell feeds what
//! `cmdr_fs` writes, never a hand-built line, and checks it hashes like the hand-built spellings
//! `markers_tests.rs` pins.

use super::markers_tests::{HEAD, assert_ships_none, context, hashes, quoted, r, shape, span};
use cmdr_fs::log_detail::{LOCAL_DETAIL_MAX_BYTES, LogDetail, LoggedText};
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

/// A real log site, as a report ships it: `list_directory_core`'s two lines for an empty folder
/// whose name holds a paren, the shape the greedy fallback can only guess at. The folder goes as
/// one token and every word after it stays. The numbers vary per machine, so they're patterns.
#[test]
fn the_listing_lines_ship_their_dir_as_one_token_and_keep_the_rest() {
    let root = crate::test_support::TestDir::new("listing-golden");
    let dir = root.join("Summer (Lake House) 2026");
    std::fs::create_dir(&dir).expect("create the empty folder");
    let host = cmdr_fs::volume::host::VolumeHost::detached();
    let (listed, logs) =
        crate::test_support::capture_logs(|| cmdr_local::reading::list_directory_core(&dir, host.listings()));
    assert!(listed.expect("list the empty folder").is_empty());

    let token = regex::escape(&r(&LogPrivate::dir_path(&dir).to_string()));
    let goldens = [
        format!(r"^list_directory_core: path={token}, entries=0, read_dir=\d+ms, total=\d+ms$"),
        format!(
            r"^list_directory_core: 0 entries at {token} \(kind=dir, perms=0o\d+, size=\d+\)\. Could be genuinely empty, permission/TCC restricted \(Full Disk Access\?\), or a cloud-sync placeholder\.$"
        ),
    ];
    let lines: Vec<_> = logs
        .iter()
        .filter(|log| log.message.starts_with("list_directory_core:"))
        .map(|log| log.file_line())
        .collect();
    assert_eq!(lines.len(), goldens.len(), "{lines:#?}");
    for (line, golden) in lines.iter().zip(goldens) {
        assert_ships_none(line, &["Lake", "House", "Summer"]);
        let shipped = r(line);
        let message = &shipped[super::markers::message_start(&shipped)..];
        assert!(
            regex::Regex::new(&golden).expect("valid golden").is_match(message),
            "{message}\n{golden}"
        );
    }
}

/// A `log_error!` message that carries a marked `detail=` field, quoted again as the Flow B note's
/// own `detail=`: the inner field's span sits two depths down inside a field inside a field, and
/// still ships as the token the log line gives it, never as escaped placeholder text.
#[test]
fn a_span_in_a_detail_inside_a_detail_ships_its_token() {
    let stderr = format!("open({FILE}): Permission denied");
    let message = format!(
        "couldn't copy: detail={:?}; tail",
        LogDetail(&stderr).marking(&[LogPrivate::file_path(FILE)]),
    );
    let note = format!("auto-send: first: copy detail={:?}", LoggedText(&message));
    assert_ships_none(&note, &["Lake", "House", "IMG", "0001"]);
    let line = format!("{HEAD}{message}");
    let (in_note, on_line) = (context().redact_line(&note), context().redact_line(&line));
    assert_eq!(hashes(&in_note), hashes(&on_line), "{in_note}\n{on_line}");
    assert_eq!(hashes(&in_note).len(), 2, "{in_note}");
}

/// The index's own lines, in each site's format string, as a report ships them. Each pins what
/// the marker buys over the line's unmarked spelling: the kind the site knows, the stat after a
/// prose path, and a bare name no regex could find.
#[test]
fn the_index_lines_ship_each_value_as_one_token_of_its_kind() {
    // A dir whose name reads like a file: only the site knows it's a folder.
    let dir = "/Users/me/Library/Containers/com.example.app";
    let reconcile = |path: &dyn std::fmt::Debug| {
        format!("{HEAD}MustScanSubDirs: reconcile complete for path={path:?} (+0 -0 ~0, 6ms, 1ms CPU)")
    };
    let golden = format!(
        "{HEAD}MustScanSubDirs: reconcile complete for path=\"$HOME/Library/<dir>/<dir>\" (+0 -0 ~0, 6ms, 1ms CPU)"
    );
    assert_eq!(shape(&reconcile(&LogPrivate::dir_path(dir))), golden);
    assert_ne!(
        shape(&reconcile(&dir)),
        golden,
        "the unmarked spelling guesses the kind"
    );

    // A prose path followed by a parenthesized word, which the greedy fallback can't tell apart.
    let root = "/Users/me/Pictures/Summer (Lake House) 2026";
    let covering = |path: &dyn std::fmt::Display| format!("{HEAD}Phases: covering {path} (Home)");
    let marked = covering(&LogPrivate::dir_path(root));
    assert_ships_none(&marked, &["Lake", "House", "Summer"]);
    let golden = format!("{HEAD}Phases: covering $HOME/Pictures/<dir> (Home)");
    assert_eq!(shape(&marked), golden);
    assert_ne!(shape(&covering(&root)), golden, "the unmarked spelling eats the phase");

    // A bare entry name, which has no path around it for a regex to claim.
    let name = "IMG Lake 0001.jpg";
    let inserted =
        |name: &dyn std::fmt::Display| format!("{HEAD}Writer: UpsertEntryV2 inserted \"{name}\" (parent_id=5) → id=9");
    let marked = inserted(&LogPrivate::new(LogKind::FileName, name));
    assert_ships_none(&marked, &["Lake", "IMG", "0001"]);
    assert_eq!(
        shape(&marked),
        format!("{HEAD}Writer: UpsertEntryV2 inserted \"<file>.jpg\" (parent_id=5) → id=9")
    );
    assert!(
        shape(&inserted(&name)).contains("Lake"),
        "the unmarked name ships whole"
    );
}
