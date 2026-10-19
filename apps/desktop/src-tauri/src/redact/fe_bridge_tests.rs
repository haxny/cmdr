//! The frontend's half of the markers: its table and body escaping match `cmdr_fs::log_private`,
//! its JSON identity keys are ones the detail redactor knows, and the exact lines its log bridge
//! writes (`src/lib/logging/bridge-redaction-cases.json`, pinned against the real bridge by
//! `log-bridge.test.ts`) ship no name and hash each value the way a Rust `LogPrivate` line does.

use super::markers_tests::{assert_ships_none, context, hashes, quoted};
use cmdr_fs::log_private::{CLOSER, LogKind, RESERVED};
use serde::Deserialize;
use std::collections::BTreeMap;

const MARKERS: &str = include_str!("../../../src/lib/logging/log-markers.json");
const CASES: &str = include_str!("../../../src/lib/logging/bridge-redaction-cases.json");
const FE_HEAD: &str = "2026-10-19T10:00:00.000+02:00 ERROR FE:fileExplorer  ";

#[derive(Deserialize)]
struct MarkerTable {
    reserved: Reserved,
    closer: String,
    openers: BTreeMap<String, String>,
    #[serde(rename = "jsonKeyKinds")]
    json_key_kinds: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Reserved {
    first: String,
    last: String,
}

#[derive(Deserialize)]
struct Cases {
    #[serde(rename = "bodyProbes")]
    body_probes: Vec<BodyProbe>,
    lines: Vec<BridgeLine>,
}

#[derive(Deserialize)]
struct BodyProbe {
    value: String,
    body: String,
}

#[derive(Deserialize)]
struct BridgeLine {
    case: String,
    line: String,
    private: Vec<String>,
    parity: Vec<Parity>,
}

#[derive(Deserialize)]
struct Parity {
    kind: String,
    value: String,
}

fn marker_table() -> MarkerTable {
    serde_json::from_str(MARKERS).expect("log-markers.json parses")
}

fn cases() -> Cases {
    serde_json::from_str(CASES).expect("bridge-redaction-cases.json parses")
}

/// The frontend's name for each kind: the `LogPrivate` constructor's.
fn kind_name(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Path => "path",
        LogKind::FilePath => "file_path",
        LogKind::DirPath => "dir_path",
        LogKind::Name => "name",
        LogKind::FileName => "file_name",
        LogKind::DirName => "dir_name",
        LogKind::Volume => "volume",
        LogKind::Host => "host",
        LogKind::User => "user",
    }
}

fn kind_named(name: &str) -> Option<LogKind> {
    LogKind::ALL.into_iter().find(|kind| kind_name(*kind) == name)
}

fn code_point(c: char) -> String {
    format!("{:04X}", c as u32)
}

#[test]
fn the_frontend_marker_table_is_the_rust_one() {
    let table = marker_table();
    assert_eq!(table.reserved.first, code_point(*RESERVED.start()));
    assert_eq!(table.reserved.last, code_point(*RESERVED.end()));
    assert_eq!(table.closer, code_point(CLOSER));
    let rust: BTreeMap<String, String> = LogKind::ALL
        .into_iter()
        .map(|kind| (kind_name(kind).to_string(), code_point(kind.opener())))
        .collect();
    assert_eq!(table.openers, rust);
}

/// Byte equality on the chars that keep a span safe: quotes, backslashes, control, bidi-format,
/// line separators, and the reserved range. (Beyond them, Rust escapes more than the frontend,
/// combining marks for one; both unescape to the same value.)
#[test]
fn the_frontend_body_escaping_matches_rust_debug_on_the_safety_set() {
    for probe in cases().body_probes {
        let quoted = format!("{:?}", probe.value);
        assert_eq!(probe.body, quoted[1..quoted.len() - 1], "{:?}", probe.value);
    }
}

/// The fallback for a typed error logged without marks (an older frontend, a rotated file): every
/// key the frontend marks is one the detail redactor tokenizes by key too.
#[test]
fn an_unmarked_identity_pair_is_still_tokenized_by_its_key() {
    let table = marker_table();
    assert!(!table.json_key_kinds.is_empty());
    for (key, kind) in &table.json_key_kinds {
        assert!(kind_named(kind).is_some(), "{key}: unknown kind {kind}");
        let json = format!(r#"{{"type":"x","{key}":"Summer (Lake House) Trip/Notes"}}"#);
        let line = format!("{FE_HEAD}Copy failed: detail={json:?}");
        assert_ships_none(&line, &["Summer", "Lake House", "Trip", "Notes"]);
    }
}

#[test]
fn a_bridge_line_ships_none_of_its_names() {
    for case in cases().lines {
        assert!(!case.line.is_empty(), "{}: the golden line is missing", case.case);
        let line = format!("{FE_HEAD}{}", case.line);
        let private: Vec<&str> = case.private.iter().map(String::as_str).collect();
        assert_ships_none(&line, &private);
    }
}

#[test]
fn a_bridge_value_hashes_like_the_same_value_from_rust() {
    let context = context();
    for case in cases().lines {
        let fe = hashes(&context.redact_line(&format!("{FE_HEAD}{}", case.line)));
        for parity in &case.parity {
            let kind =
                kind_named(&parity.kind).unwrap_or_else(|| panic!("{}: unknown kind {}", case.case, parity.kind));
            let rust_line = format!("{FE_HEAD}value={}", quoted(kind, &parity.value));
            let rust = hashes(&context.redact_line(&rust_line));
            assert!(!rust.is_empty(), "{}: no token for {rust_line}", case.case);
            for hash in &rust {
                assert!(
                    fe.contains(hash),
                    "{}: {} {:?} hashes {rust:?} from Rust, the bridge line has {fe:?}",
                    case.case,
                    parity.kind,
                    parity.value
                );
            }
        }
    }
}
