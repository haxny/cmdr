use super::{LATEST_FOLDER_NAME, RawSnapshot, snapshot_list};
use rustic_core::Id;
use rustic_core::jiff::Timestamp;
use rustic_core::jiff::tz::TimeZone;

fn raw(id_hex: &str, time: &str) -> RawSnapshot {
    RawSnapshot {
        id: id_hex.parse::<Id>().expect("valid hex id"),
        time: time.parse::<Timestamp>().expect("valid RFC 3339 time"),
        hostname: "m3".to_string(),
        paths: vec!["/Users/david".to_string()],
        tags: vec![],
    }
}

/// Pads a short hex prefix out to a full 64-char id.
fn hex(prefix: &str) -> String {
    format!("{prefix:0<64}")
}

fn stockholm() -> TimeZone {
    TimeZone::get("Europe/Stockholm").expect("tzdb has Stockholm")
}

fn names(list: &super::SnapshotList) -> Vec<&str> {
    list.snapshots().iter().map(|s| s.folder_name.as_str()).collect()
}

#[test]
fn a_folder_name_is_local_time_plus_the_short_id() {
    let list = snapshot_list(vec![raw(&hex("a1b2c3d4e5"), "2026-10-12T19:00:03Z")], &stockholm());
    // 19:00 UTC is 21:00 in Stockholm (CEST, +02:00) on that date.
    assert_eq!(names(&list), vec!["2026-10-12 21.00.03 (a1b2c3d4)"]);
}

#[test]
fn the_list_is_oldest_first_and_name_order_matches() {
    let list = snapshot_list(
        vec![
            raw(&hex("cc"), "2026-10-14T08:00:00Z"),
            raw(&hex("aa"), "2025-01-02T08:00:00Z"),
            raw(&hex("bb"), "2026-03-01T08:00:00Z"),
        ],
        &stockholm(),
    );
    let names = names(&list);
    let mut by_name = names.clone();
    by_name.sort_unstable();
    assert_eq!(names, by_name, "sorting by name is sorting by time");
    assert!(names[0].starts_with("2025-01-02"));
    assert!(names[2].starts_with("2026-10-14"));
}

#[test]
fn two_snapshots_in_the_same_second_get_distinct_names() {
    let list = snapshot_list(
        vec![
            raw(&hex("aaaa0001"), "2026-10-12T19:00:03.100Z"),
            raw(&hex("bbbb0002"), "2026-10-12T19:00:03.900Z"),
        ],
        &stockholm(),
    );
    assert_eq!(
        names(&list),
        vec!["2026-10-12 21.00.03 (aaaa0001)", "2026-10-12 21.00.03 (bbbb0002)"]
    );
}

#[test]
fn a_short_id_clash_in_the_same_second_lengthens_until_unique() {
    // Two ids sharing their first eight hex digits, taken in the same second: the
    // eight-digit short form alone would name both folders the same.
    let list = snapshot_list(
        vec![
            raw(&hex("a1b2c3d4e5"), "2026-10-12T19:00:03Z"),
            raw(&hex("a1b2c3d4f6"), "2026-10-12T19:00:03Z"),
            raw(&hex("ffff0000"), "2026-10-12T19:00:03Z"),
        ],
        &stockholm(),
    );
    let names = names(&list);
    assert!(names.contains(&"2026-10-12 21.00.03 (a1b2c3d4e)"));
    assert!(names.contains(&"2026-10-12 21.00.03 (a1b2c3d4f)"));
    // The lengthening is per second: an id with no clash keeps the short form.
    assert!(names.contains(&"2026-10-12 21.00.03 (ffff0000)"));
}

#[test]
fn the_same_short_id_in_different_seconds_keeps_the_short_form() {
    let list = snapshot_list(
        vec![
            raw(&hex("a1b2c3d4e5"), "2026-10-12T19:00:03Z"),
            raw(&hex("a1b2c3d4f6"), "2026-10-12T19:00:04Z"),
        ],
        &stockholm(),
    );
    assert_eq!(
        names(&list),
        vec!["2026-10-12 21.00.03 (a1b2c3d4)", "2026-10-12 21.00.04 (a1b2c3d4)"]
    );
}

#[test]
fn a_name_is_one_valid_path_component() {
    let list = snapshot_list(vec![raw(&hex("ab"), "2026-10-12T19:00:03Z")], &stockholm());
    let name = &list.snapshots()[0].folder_name;
    assert!(!name.contains('/') && !name.contains(':'), "no separators on any OS");
}

#[test]
fn latest_is_the_newest_by_time_whatever_the_input_order() {
    let list = snapshot_list(
        vec![
            raw(&hex("bb"), "2026-10-14T08:00:00Z"),
            raw(&hex("aa"), "2026-10-15T08:00:00Z"),
            raw(&hex("cc"), "2026-10-13T08:00:00Z"),
        ],
        &stockholm(),
    );
    let latest = list.latest().expect("a non-empty list has a latest");
    assert!(latest.id.starts_with("aa"));
}

#[test]
fn latest_breaks_a_same_instant_tie_by_id() {
    // Deterministic, so the `latest` folder doesn't flip between refreshes.
    let list = snapshot_list(
        vec![
            raw(&hex("bb"), "2026-10-14T08:00:00Z"),
            raw(&hex("aa"), "2026-10-14T08:00:00Z"),
        ],
        &stockholm(),
    );
    assert!(list.latest().expect("latest").id.starts_with("bb"));
}

#[test]
fn find_resolves_a_dated_name_and_latest() {
    let list = snapshot_list(
        vec![
            raw(&hex("aa"), "2026-10-13T08:00:00Z"),
            raw(&hex("bb"), "2026-10-14T08:00:00Z"),
        ],
        &stockholm(),
    );
    let first_name = list.snapshots()[0].folder_name.clone();
    assert!(list.find(&first_name).expect("dated").id.starts_with("aa"));
    assert!(list.find(LATEST_FOLDER_NAME).expect("latest").id.starts_with("bb"));
    assert!(list.find("2026-10-13 nope").is_none());
}

#[test]
fn latest_never_collides_with_a_dated_name() {
    let list = snapshot_list(vec![raw(&hex("aa"), "2026-10-13T08:00:00Z")], &stockholm());
    assert!(list.snapshots().iter().all(|s| s.folder_name != LATEST_FOLDER_NAME));
}

#[test]
fn an_empty_repo_has_no_latest() {
    let list = snapshot_list(vec![], &stockholm());
    assert!(list.snapshots().is_empty());
    assert!(list.latest().is_none());
    assert!(list.find(LATEST_FOLDER_NAME).is_none());
}

#[test]
fn a_snapshot_carries_its_host_paths_tags_and_time() {
    let mut snapshot = raw(&hex("aa"), "2026-10-13T08:00:00Z");
    snapshot.tags = vec!["daily".to_string()];
    let list = snapshot_list(vec![snapshot], &stockholm());
    let s = &list.snapshots()[0];
    assert_eq!(s.hostname, "m3");
    assert_eq!(s.paths, vec!["/Users/david".to_string()]);
    assert_eq!(s.tags, vec!["daily".to_string()]);
    assert_eq!(
        s.time,
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_791_878_400)
    );
    assert_eq!(s.id.len(), 64);
}
