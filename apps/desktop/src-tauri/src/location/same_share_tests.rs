//! The proof rules: GUID plus share name groups, GUID alone never does, and only
//! the same account redirects.

use super::*;
use crate::location::resolve::IndexSources;
use crate::mcp::resources::volumes::VolumeKind;
use cmdr_fs::volume::ConnectionState;

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_BOX: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";
const ZERO: &str = "00000000-0000-0000-0000-000000000000";

fn share(id: &str, root: &str, live: bool, identity: ShareIdentity) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live,
        connection_state: Some(if live {
            ConnectionState::Direct
        } else {
            ConnectionState::Saved
        }),
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: Some(identity),
    }
}

fn facts(volumes: Vec<VolumeFact>) -> VolumeFacts {
    VolumeFacts {
        volumes,
        mount_areas: vec!["/Volumes".to_string()],
    }
}

fn david(guid: Option<&str>, share_name: &str) -> ShareIdentity {
    ShareIdentity::new(guid, share_name, Some("david"))
}

#[test]
fn one_guid_and_one_share_name_ignoring_case_is_one_share() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            true,
            david(Some(&BOX.to_uppercase()), "NASPI"),
        ),
    ]);

    assert_eq!(facts.same_share_as("ts"), ["lan"]);
    assert_eq!(facts.same_share_as("lan"), ["ts"]);
}

/// The GUID names the BOX: two shares on one NAS are two places.
#[test]
fn one_guid_with_two_share_names_is_two_shares() {
    let facts = facts(vec![
        share("naspi", "/Volumes/naspi", true, david(Some(BOX), "naspi")),
        share("media", "/Volumes/media", true, david(Some(BOX), "media")),
    ]);

    assert!(facts.same_share_as("naspi").is_empty());
}

#[test]
fn a_missing_or_all_zero_guid_proves_nothing() {
    let facts = facts(vec![
        share("a", "/Volumes/naspi", true, david(None, "naspi")),
        share("b", "/Volumes/naspi-1", true, david(None, "naspi")),
        share("c", "/Volumes/naspi-2", true, david(Some(ZERO), "naspi")),
        share("d", "/Volumes/naspi-3", true, david(Some(ZERO), "naspi")),
    ]);

    for id in ["a", "b", "c", "d"] {
        assert!(facts.same_share_as(id).is_empty(), "{id} has no proven sibling");
    }
}

#[test]
fn two_servers_with_one_share_name_are_two_shares() {
    let facts = facts(vec![
        share("home", "/Volumes/public", true, david(Some(BOX), "public")),
        share("office", "/Volumes/public-1", true, david(Some(OTHER_BOX), "public")),
    ]);

    assert!(facts.same_share_as("home").is_empty());
}

/// ❗ Another account can see different files on the same share, so it's a
/// `sameShareAs` hint and never answers for the volume asked.
#[test]
fn the_same_share_under_two_accounts_is_grouped_but_never_redirected() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            true,
            ShareIdentity::new(Some(BOX), "naspi", Some("dori")),
        ),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers".to_string(),
        },
    )
    .expect("resolves");

    assert_eq!(facts.same_share_as("ts"), ["lan"]);
    assert_eq!(facts.readers(&asked).len(), 1, "only the volume asked reads");
    assert_eq!(facts.preferred_member("ts").map(|v| v.id.as_str()), Some("ts"));
}

/// A not-connected miss names its proven siblings: here another account's, which
/// couldn't answer in its place.
#[test]
fn a_volume_not_connected_miss_names_the_same_share() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share(
            "lan",
            "/Volumes/naspi-1",
            false,
            ShareIdentity::new(Some(BOX), "naspi", Some("dori")),
        ),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers".to_string(),
        },
    )
    .expect("resolves");

    let miss = crate::location::miss::volume_not_connected(&facts, &asked);

    assert_eq!(miss.same_share_as, Some(vec!["lan".to_string()]));
}

/// A guest sign-in is no account, whichever way it was spelled.
#[test]
fn guest_sessions_are_one_account() {
    let guest = |guid| ShareIdentity::new(guid, "public", Some("Guest"));
    assert!(guest(Some(BOX)).redirectable(&ShareIdentity::new(Some(BOX), "public", None)));
}

#[test]
fn a_read_is_re_addressed_onto_each_same_account_member_with_via() {
    let facts = facts(vec![
        share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi")),
        share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi")),
    ]);
    let asked = crate::location::resolve::resolve(
        &facts,
        &crate::location::resolve::LocationInput {
            volume_id: Some("ts".to_string()),
            path: "/Volumes/naspi/papers/invoices paid".to_string(),
        },
    )
    .expect("resolves");

    let readers = facts.readers(&asked);

    assert_eq!(readers[0], asked, "the volume asked comes first");
    assert_eq!(readers[1].location.volume_id, "lan");
    assert_eq!(readers[1].location.path, "/Volumes/naspi-1/papers/invoices paid");
    assert_eq!(readers[1].via.as_deref(), Some("ts"));
    assert!(readers[1].live);
}

#[test]
fn the_preferred_member_is_live_then_indexed_then_freshest_on_disk() {
    let mut ts = share("ts", "/Volumes/naspi", false, david(Some(BOX), "naspi"));
    ts.index.stored = true;
    ts.index.stored_as_of = Some(100);
    let mut old = share("old", "/Volumes/naspi-old", false, david(Some(BOX), "naspi"));
    old.index.stored = true;
    old.index.stored_as_of = Some(50);
    let lan = share("lan", "/Volumes/naspi-1", true, david(Some(BOX), "naspi"));

    let offline = facts(vec![old.clone(), ts.clone()]);
    assert_eq!(offline.preferred_member("old").map(|v| v.id.as_str()), Some("ts"));

    let with_lan = facts(vec![ts, old, lan]);
    assert_eq!(with_lan.preferred_member("ts").map(|v| v.id.as_str()), Some("lan"));
    assert_eq!(with_lan.preferred_member("lan").map(|v| v.id.as_str()), Some("lan"));
}
