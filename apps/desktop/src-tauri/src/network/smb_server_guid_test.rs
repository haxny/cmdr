use super::*;
use crate::network::known_shares::{AuthOptions, ConnectionMode};

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";

fn row(address: &str, share: &str, guid: Option<&str>) -> KnownNetworkShare {
    KnownNetworkShare {
        server_name: address.to_string(),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-26T10:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: Some("david".to_string()),
        address: Some(address.to_string()),
        port: None,
        volume_id: Some(format!("smb-{address}-{share}")),
        mount_path: Some(format!("/Volumes/{share}")),
        pinned: true,
        server_guid: guid.map(str::to_string),
    }
}

/// Two rows of one share name, one without a GUID: only that one is dialed. A row
/// already holding a GUID never is, since a probe can't change it.
#[test]
fn the_probe_dials_only_rows_missing_a_guid_that_another_row_shares_a_name_with() {
    let rows = [
        row("100.127.48.122", "naspi", Some(BOX)),
        row("192.168.1.111", "NASPI", None),
        row("192.168.1.111", "media", None),
    ];

    let picked: Vec<&str> = rows_to_probe(&rows).iter().map(|r| r.share_name.as_str()).collect();

    assert_eq!(picked, ["NASPI"]);
}

#[test]
fn a_lone_share_or_one_whose_rows_all_hold_guids_is_never_probed() {
    assert!(rows_to_probe(&[row("192.168.1.111", "naspi", None)]).is_empty());
    assert!(rows_to_probe(&[row("a", "naspi", Some(BOX)), row("b", "naspi", Some(BOX))]).is_empty());
}

#[test]
fn a_zero_guid_is_probed_like_a_missing_one() {
    let rows = [
        row("a", "naspi", Some("00000000-0000-0000-0000-000000000000")),
        row("b", "naspi", Some(BOX)),
    ];
    assert_eq!(rows_to_probe(&rows).len(), 1);
}

/// A direct session coming up records the GUID its NEGOTIATE answered, read off
/// the session's diagnostics with no I/O. A guest session only fills, and a share
/// with no saved row keeps it in memory for the row a mount saves later.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_direct_session_records_its_server_guid() {
    let volume = cmdr_smb::volume::testing::make_docker_volume().await;
    let volume_id = "smb-integration-guid-record-test";

    record_session_guid(&volume, volume_id, "public", Some("Guest")).await;

    let live = live_share(volume_id).expect("the session's GUID is kept");
    assert_eq!(live.proof, GuidProof::Unauthenticated, "a guest session only fills");
    assert_eq!(live.share_name, "public");
    assert_eq!(live.username, None);
    let probed = cmdr_smb::connection::probe_server_guid(
        "127.0.0.1",
        cmdr_smb::volume::testing::guest_port(),
        Duration::from_secs(3),
    )
    .await
    .expect("the fixture answers");
    assert_eq!(
        Some(live.server_guid),
        ServerGuid::from_negotiated(probed),
        "the session and the probe name one server"
    );
}

/// ❗ A model's read must never dial a host: the probe and the session recorder
/// are reachable from the network wiring only. A source scan, because nothing in
/// the type system tells a tool handler from any other caller.
#[test]
fn no_tool_handler_reaches_the_probe() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for dir in ["agent", "mcp", "location"] {
        let mut stack = vec![root.join(dir)];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).expect("readable source dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("readable source");
                    if [
                        "probe_server_guid",
                        "schedule_probe",
                        "adopt_live_guid",
                        "record_session_guid",
                    ]
                    .iter()
                    .any(|name| text.contains(name))
                    {
                        offenders.push(path.display().to_string());
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "these reach the GUID probe: {offenders:?}");
}
