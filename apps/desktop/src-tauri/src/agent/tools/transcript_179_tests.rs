//! The volume facts of prod conversation 179, replayed: the acceptance test for
//! volume-aware tools (`docs/specs/ask-cmdr-volume-aware-tools-plan.md`).
//!
//! One NAS, two ids. The Tailscale share is saved and not connected, last root
//! `/Volumes/naspi`; the LAN share is live at `/Volumes/naspi-1` with no drive
//! index. Both rows record one share name and one username. The model guessed
//! `/Volumes/naspi/papers`, a dead mount point, and read `root`'s fresh, empty,
//! "authoritative" answer three times.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::agent::tools::read::listing::{
    GatePolicy, LIVE_READ, ListOptions, LiveAsk, VolumeBlock, VolumeView, answer_live,
};
use crate::location::miss::LocationMissResult;
use crate::location::resolve::{IndexSources, LocationInput, VolumeFact, VolumeFacts, resolve};
use crate::mcp::resources::volumes::VolumeKind;
use crate::test_support::TestDir;
use cmdr_fs::volume::ConnectionState;

const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";

fn conversation_179() -> VolumeFacts {
    conversation_179_with_lan_at("/Volumes/naspi-1")
}

/// The same facts with the LAN share mounted at `lan_root`: a temp dir standing in
/// for `/Volumes/naspi-1`, so a live read has a real folder to list.
fn conversation_179_with_lan_at(lan_root: &str) -> VolumeFacts {
    // The Tailscale share's index is on disk; the LAN share has none at all.
    let share = |id: &str, server: &str, root: &str, state: ConnectionState, stored: bool| VolumeFact {
        id: id.to_string(),
        name: format!("naspi on {server}"),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live: state != ConnectionState::Saved,
        connection_state: Some(state),
        earlier_roots: Vec::new(),
        index: IndexSources {
            stored,
            ..IndexSources::none(id)
        },
    };
    VolumeFacts {
        volumes: vec![
            VolumeFact {
                id: "root".to_string(),
                name: "Macintosh HD".to_string(),
                kind: VolumeKind::Local,
                root: "/".to_string(),
                live: true,
                connection_state: None,
                earlier_roots: Vec::new(),
                index: IndexSources {
                    live: true,
                    stored: true,
                    ..IndexSources::none("root")
                },
            },
            share(
                TAILSCALE,
                "100.127.48.122",
                "/Volumes/naspi",
                ConnectionState::Saved,
                true,
            ),
            share(LAN, "naspolya", lan_root, ConnectionState::Direct, false),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// The transcript's bare guess no longer reads as an empty, fresh, authoritative
/// folder: it's a typed miss whose next call is ready on the LAN share.
#[test]
fn the_bare_guess_under_the_dead_mount_is_a_miss_pointing_at_the_live_share() {
    let input = LocationInput {
        volume_id: None,
        path: "/Volumes/naspi/papers".to_string(),
    };
    let miss = resolve(&conversation_179(), &input).expect_err("nothing live is mounted at /Volumes/naspi");
    let wire: Value = serde_json::to_value(LocationMissResult::from(miss)).expect("serializes");

    assert_eq!(wire["miss"], "noVolumeAtPath");
    assert_eq!(wire["location"]["path"], "/Volumes/naspi/papers");
    assert!(wire.get("authoritative").is_none() && wire.get("children").is_none());
    let nearby = wire["nearby"].as_array().expect("nearby");
    let lan = nearby
        .iter()
        .find(|n| n["volumeId"] == LAN)
        .expect("the live LAN share is nearby");
    assert_eq!(lan["candidate"]["volumeId"], LAN);
    assert_eq!(lan["candidate"]["path"], "/Volumes/naspi-1/papers");
    assert_eq!(nearby[0]["volumeId"], LAN, "the live share leads");
    assert_eq!(wire["sameShareAs"], serde_json::json!([]));
}

/// The LAN share has no drive index at all, so `invoices paid` on it answers from
/// one live read of the share: the folder the user actually has connected is
/// listable, indexed or not.
#[tokio::test]
async fn the_unindexed_lan_share_lists_invoices_paid_live() {
    let mount = TestDir::new("conversation-179-naspi-1");
    let invoices = mount.join("papers/Rymdskottkärra AB/invoices paid");
    std::fs::create_dir_all(invoices.join("2025")).expect("dirs");
    std::fs::write(invoices.join("hetzner-2026-09.pdf"), b"%PDF").expect("file");
    let facts = conversation_179_with_lan_at(&mount.to_string_lossy());
    let path = invoices.to_string_lossy().into_owned();
    let resolved = resolve(
        &facts,
        &LocationInput {
            volume_id: Some(LAN.to_string()),
            path: path.clone(),
        },
    )
    .expect("the LAN share holds it");
    assert!(resolved.live && !resolved.index.live, "live, with no index to answer");

    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "naspi",
        &*mount,
        crate::volume_host::host(),
    ));
    let view = VolumeView {
        enabled: false,
        freshness: None,
        block: VolumeBlock::new(LAN.to_string(), None),
        walked_roots: Vec::new(),
    };
    let wire = answer_live(LiveAsk {
        resolved: &resolved,
        facts: &facts,
        opts: &ListOptions::default(),
        view: &view,
        volume: Some(volume),
        cancel: &CancellationToken::new(),
        cfg: LIVE_READ,
        policy: GatePolicy {
            fda_pending: false,
            is_protected: |_| false,
            denial_is_privacy: |_| false,
        },
    })
    .await
    .expect("answers");

    assert_eq!(wire["coverage"]["source"], "live");
    assert_eq!(wire["location"]["volumeId"], LAN);
    assert_eq!(wire["location"]["path"], path);
    let names: Vec<&str> = wire["children"]
        .as_array()
        .expect("children")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(names, ["2025", "hetzner-2026-09.pdf"]);
}
