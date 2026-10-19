//! Only our own crates' DEBUG and TRACE reach a report, in both ZIP pipelines. The verdict is
//! the provenance tag the file chain writes (`logging/provenance.rs`); continuation lines
//! follow their header, and anything without a parseable header drops.

use super::{read_zip_entries, sample_manifest, test_redaction};
use crate::error_reporter::BundleScope;
use crate::error_reporter::bundle_builder::{build_bundle_streaming, load_and_filter_log_file};
use crate::test_support::TestDir;
use chrono::{DateTime, Utc};
use std::time::SystemTime;

fn timestamp(at: DateTime<Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string()
}

/// One log file with every provenance shape, written a minute ago.
fn mixed_log(now: DateTime<Utc>) -> String {
    let t = timestamp(now - chrono::Duration::minutes(1));
    [
        "orphan continuation before any header".to_string(),
        format!("{t} DEBUG ·copy  ours-short-target"),
        format!("{t} DEBUG copy  foreign-same-target"),
        format!("{t} DEBUG mdns_sd::service_daemon  foreign multi-line record"),
        "foreign continuation line".to_string(),
        format!(
            "{t} DEBUG smb2::client::tree  tree: exclusive delete of Photos/2026/Summer (Lake House)/x.jpg refused"
        ),
        format!("{t} DEBUG ·FE:viewer  ours-fe"),
        format!("{t} DEBUG indexing::writer  pre-upgrade-debug"),
        format!("{t} WARN  copy  pre-upgrade-warn"),
        format!(
            "{t} INFO  smb2::client::durable  durable: Photos/Summer (Lake House)/x.jpg reclaimed on the new session"
        ),
        format!("{t} ERROR ·copy  ours-error"),
        "ours continuation".to_string(),
        format!("{t} NEW unparseable header"),
        "unparseable continuation".to_string(),
    ]
    .join("\n")
        + "\n"
}

fn assert_only_shippable_lines(body: &str) {
    for kept in [
        "ours-short-target",
        "ours-fe",
        "pre-upgrade-warn",
        "ours-error",
        "ours continuation",
    ] {
        assert!(body.contains(kept), "{kept:?} was dropped:\n{body}");
    }
    for dropped in [
        "orphan continuation",
        "foreign-same-target",
        "foreign multi-line",
        "foreign continuation",
        "exclusive delete",
        "Lake House",
        "pre-upgrade-debug",
        "durable",
        "unparseable",
    ] {
        assert!(!body.contains(dropped), "{dropped:?} shipped:\n{body}");
    }
}

#[test]
fn the_streaming_pipeline_ships_only_our_debug() {
    let dir = TestDir::new("provenance-streaming");
    let log = dir.join("cmdr.log");
    let now = Utc::now();
    std::fs::write(&log, mixed_log(now)).expect("write log");

    let bundle = build_bundle_streaming(
        "ERR-PROV1".to_string(),
        sample_manifest(),
        vec![log],
        now - chrono::Duration::hours(1),
        SystemTime::now(),
        &test_redaction(),
    )
    .expect("build_bundle_streaming");

    let entries = read_zip_entries(&bundle.zip_bytes);
    assert_only_shippable_lines(entries.get("logs/cmdr.log").expect("log entry"));
    assert_eq!(bundle.total_redacted_lines, 5, "the count covers shipped lines only");
}

#[test]
fn the_window_pipeline_ships_only_our_debug() {
    let dir = TestDir::new("provenance-window");
    let log = dir.join("cmdr.log");
    let now = Utc::now();
    std::fs::write(&log, mixed_log(now)).expect("write log");

    let (lines, _) = load_and_filter_log_file(
        &log,
        BundleScope::Window { first_error_at: now },
        now,
        SystemTime::now(),
        &test_redaction(),
    )
    .expect("file should be loaded");

    assert_only_shippable_lines(&lines.join("\n"));
}
