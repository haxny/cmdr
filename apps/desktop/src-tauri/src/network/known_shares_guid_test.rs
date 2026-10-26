//! The server GUID on a share row, and the proof it was learned with: which
//! writer may set it, and that every refresh of the row keeps both.

use super::*;

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_BOX: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";

fn lan_row() -> Vec<KnownNetworkShare> {
    vec![mounted(
        "192.168.1.111",
        "192.168.1.111",
        "naspi",
        Some("david"),
        "smb-lan",
    )]
}

/// ❗ **A probe's answer only FILLS a missing GUID.** Whatever answers on a saved
/// address isn't proven to be the share's server, so it can't replace one a
/// signed-in session recorded, and what it fills is recorded as unproven.
#[test]
fn an_unauthenticated_guid_fills_a_missing_one_and_never_overwrites() {
    let mut rows = lan_row();

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Unauthenticated),
        Some(true)
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Unauthenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", OTHER_BOX, GuidProof::Unauthenticated),
        Some(false),
        "a probe finding a row that holds a GUID writes nothing"
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
}

/// ❗ **A signed-in session's GUID overwrites**: a NAS restored or replaced mints a
/// new one, and the latest authenticated answer is the truth.
#[test]
fn an_authenticated_guid_overwrites_the_stored_one() {
    let mut rows = lan_row();
    rows[0].server_guid = Some(BOX.to_string());

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", OTHER_BOX, GuidProof::Authenticated),
        Some(true)
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(OTHER_BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);
    assert_eq!(rows[0].username.as_deref(), Some("david"), "no other field moves");
    assert_eq!(
        record_guid_in(&mut rows, "smb-nope", BOX, GuidProof::Authenticated),
        None
    );
}

/// A signed-in session answering the GUID a probe filled proves it: the row's
/// proof moves up, and a later probe can't move it back down.
#[test]
fn an_authenticated_session_upgrades_a_probed_guids_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Unauthenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Authenticated),
        Some(true),
        "the proof changed, so the row is written"
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Authenticated),
        Some(false)
    );
    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Unauthenticated),
        Some(false)
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);
}

/// A stored all-zero GUID is no GUID, so a probe may fill it.
#[test]
fn a_stored_zero_guid_counts_as_missing() {
    let mut rows = vec![mounted("192.168.1.111", "192.168.1.111", "naspi", None, "smb-lan")];
    rows[0].server_guid = Some("00000000-0000-0000-0000-000000000000".to_string());

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Unauthenticated),
        Some(true)
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
}

/// A later mount, or a reconnect through the share list, files a row with no
/// GUID; the share's recorded one stays, with its proof.
#[test]
fn a_remount_keeps_the_recorded_guid_and_its_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, GuidProof::Authenticated);

    upsert_share_row(
        &mut rows,
        mounted("192.168.1.111", "192.168.1.111", "naspi", Some("david"), "smb-lan"),
        &[],
    );

    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    let mut reconnect = mounted("192.168.1.111", "192.168.1.111", "naspi", Some("david"), "smb-lan");
    keep_recorded_guid(&mut reconnect, &mut rows[0]);
    assert_eq!(reconnect.server_guid.as_deref(), Some(BOX));
    assert_eq!(reconnect.server_guid_proof, GuidProof::Authenticated);
}

/// `known-shares.json` from before Cmdr kept the proof: its GUID reads as one no
/// sign-in vouched for, and the proof is written beside it from then on.
#[test]
fn a_row_saved_before_the_proof_reads_as_unauthenticated() {
    let json = format!(
        r#"{{"serverName":"naspolya","shareName":"naspi","protocol":"smb","lastConnectedAt":"2026-01-01T00:00:00Z","lastConnectionMode":"credentials","lastKnownAuthOptions":"credentials_only","username":"david","serverGuid":"{BOX}"}}"#
    );

    let mut row: KnownNetworkShare = serde_json::from_str(&json).expect("an older row reads");

    assert_eq!(row.server_guid.as_deref(), Some(BOX));
    assert_eq!(row.server_guid_proof, GuidProof::Unauthenticated);

    row.server_guid_proof = GuidProof::Authenticated;
    let written = serde_json::to_value(&row).expect("serializes");
    assert_eq!(written["serverGuidProof"], "authenticated");
}
