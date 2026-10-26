//! The volume facts of prod conversation 179, replayed: the acceptance test for
//! volume-aware tools (`docs/specs/ask-cmdr-volume-aware-tools-plan.md`).
//!
//! One NAS, two ids. The Tailscale share is saved and not connected, last root
//! `/Volumes/naspi`; the LAN share is live at `/Volumes/naspi-1` with no drive
//! index. Both rows record one share name and one username. The model guessed
//! `/Volumes/naspi/papers`, a dead mount point, and read `root`'s fresh, empty,
//! "authoritative" answer three times.

use serde_json::Value;

use crate::location::miss::LocationMissResult;
use crate::location::resolve::{IndexSources, LocationInput, VolumeFact, VolumeFacts, resolve};
use crate::mcp::resources::volumes::VolumeKind;
use cmdr_fs::volume::ConnectionState;

const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";

fn conversation_179() -> VolumeFacts {
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
                    volume_id: "root".to_string(),
                    live: true,
                    stored: true,
                },
            },
            share(
                TAILSCALE,
                "100.127.48.122",
                "/Volumes/naspi",
                ConnectionState::Saved,
                true,
            ),
            share(LAN, "naspolya", "/Volumes/naspi-1", ConnectionState::Direct, false),
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
