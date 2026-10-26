//! `important_folders` and `folder_importance` over real importance stores and
//! injected volume facts: the rows a model gets are locations it can hand straight
//! to `list_dir`, and a folder's importance is read from ONE volume's store.

use std::path::Path;

use cmdr_fs::volume::ConnectionState;
use cmdr_index::importance::PathClass;
use serde_json::{Value, json};

use super::*;
use crate::location::resolve::{IndexSources, VolumeFact};
use crate::mcp::resources::importance::tests::{low_score_signals, scored_signals, write_db};
use crate::mcp::resources::volumes::VolumeKind;

const NOW: u64 = 1_000_000;
const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
const LAN: &str = "smb-192-168-1-111-445-naspi-david";
/// A share scored under an id no listing row carries (the Bonjour address).
const BONJOUR: &str = "smb-naspolya-smb-tcp-local-4-naspi-david";

fn volume(id: &str, kind: VolumeKind, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        root: root.to_string(),
        live,
        connection_state: match (kind, live) {
            (VolumeKind::Local, _) => None,
            (_, true) => Some(ConnectionState::Direct),
            (_, false) => Some(ConnectionState::Saved),
        },
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
    }
}

/// The boot disk, the NAS live over the LAN at `/Volumes/naspi-1`, and the same NAS
/// saved over Tailscale at its last root `/Volumes/naspi`. No row for `BONJOUR`.
fn facts() -> VolumeFacts {
    VolumeFacts {
        volumes: vec![
            volume("root", VolumeKind::Local, "/", true),
            volume(LAN, VolumeKind::Smb, "/Volumes/naspi-1", true),
            volume(TAILSCALE, VolumeKind::Smb, "/Volumes/naspi", false),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

fn important(dir: &Path, params: Value) -> Value {
    important_folders_on(dir, &facts(), &params, NOW).expect("answers")
}

fn locations(result: &Value) -> Vec<(String, String)> {
    result["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .map(|f| {
            (
                f["location"]["volumeId"].as_str().expect("volumeId").to_string(),
                f["location"]["path"].as_str().expect("path").to_string(),
            )
        })
        .collect()
}

fn loc(volume_id: &str, path: &str) -> (String, String) {
    (volume_id.to_string(), path.to_string())
}

#[test]
fn every_row_is_a_location_on_its_volumes_root() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        LAN,
        &[("/papers/x", scored_signals(PathClass::UserContent, NOW))],
        NOW,
    );
    write_db(
        dir.path(),
        TAILSCALE,
        &[("/photos", scored_signals(PathClass::UserContent, NOW))],
        NOW,
    );
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/Projects", scored_signals(PathClass::ProjectRoot, NOW))],
        NOW,
    );

    let mut got = locations(&important(dir.path(), json!({})));
    got.sort();
    assert_eq!(
        got,
        [
            loc("root", "/Users/test/Projects"),
            loc(TAILSCALE, "/Volumes/naspi/photos"),
            loc(LAN, "/Volumes/naspi-1/papers/x"),
        ],
        "a share's store paths join onto its root (the last one, offline); the boot disk's are already absolute"
    );
}

/// "What matters on the NAS": a `volumeId` alone ranks that whole volume, connected
/// or not.
#[test]
fn a_volume_id_alone_ranks_that_volume() {
    let dir = tempfile::tempdir().expect("temp dir");
    let rows = [
        ("/papers", scored_signals(PathClass::UserContent, NOW)),
        ("/photos", scored_signals(PathClass::UserContent, NOW)),
    ];
    write_db(dir.path(), LAN, &rows, NOW);
    write_db(dir.path(), TAILSCALE, &rows, NOW);
    write_db(
        dir.path(),
        "root",
        &[("/Users/test/Projects", scored_signals(PathClass::ProjectRoot, NOW))],
        NOW,
    );

    let lan = important(dir.path(), json!({ "volumeId": LAN }));
    assert_eq!(
        locations(&lan),
        [loc(LAN, "/Volumes/naspi-1/papers"), loc(LAN, "/Volumes/naspi-1/photos")]
    );
    assert!(lan.get("under").is_none());

    let offline = important(dir.path(), json!({ "volumeId": TAILSCALE, "threshold": 0.0 }));
    assert_eq!(
        locations(&offline),
        [
            loc(TAILSCALE, "/Volumes/naspi/papers"),
            loc(TAILSCALE, "/Volumes/naspi/photos")
        ]
    );
}

#[test]
fn a_volume_that_cant_be_placed_yields_no_rows_and_is_named_unplaced() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_db(
        dir.path(),
        BONJOUR,
        &[("/papers", scored_signals(PathClass::ProjectRoot, NOW))],
        NOW,
    );
    write_db(dir.path(), LAN, &[("/papers", low_score_signals())], NOW);

    let result = important(dir.path(), json!({ "top": 5 }));
    assert_eq!(locations(&result), [loc(LAN, "/Volumes/naspi-1/papers")]);
    let overview = result["volumes"].as_array().expect("volumes");
    let bonjour = overview
        .iter()
        .find(|v| v["volumeId"] == BONJOUR)
        .expect("the unplaced store is still named");
    assert_eq!(bonjour["unplaced"], true);
    let lan = overview.iter().find(|v| v["volumeId"] == LAN).expect("lan");
    assert!(lan.get("unplaced").is_none(), "a placed volume carries no flag");

    let named = important(dir.path(), json!({ "volumeId": BONJOUR }));
    assert_eq!(named["miss"], "unknownVolume", "its rows have no root to stand on");
}

#[test]
fn under_takes_a_location_and_reads_only_that_volume() {
    let dir = tempfile::tempdir().expect("temp dir");
    let papers = [
        ("/papers", scored_signals(PathClass::UserContent, NOW)),
        ("/papers/invoices", scored_signals(PathClass::UserContent, NOW)),
        ("/docs", scored_signals(PathClass::UserContent, NOW)),
    ];
    write_db(dir.path(), LAN, &papers, NOW);
    write_db(dir.path(), TAILSCALE, &papers, NOW);

    for params in [
        json!({ "under": "/Volumes/naspi-1/papers", "volumeId": LAN }),
        json!({ "under": "/Volumes/naspi-1/papers" }),
    ] {
        let result = important(dir.path(), params.clone());
        assert_eq!(
            locations(&result),
            [
                loc(LAN, "/Volumes/naspi-1/papers"),
                loc(LAN, "/Volumes/naspi-1/papers/invoices"),
            ],
            "{params}"
        );
        assert_eq!(
            result["under"],
            json!({ "volumeId": LAN, "path": "/Volumes/naspi-1/papers" })
        );
    }

    let offline = important(
        dir.path(),
        json!({ "under": "/Volumes/naspi/papers/invoices", "volumeId": TAILSCALE }),
    );
    assert_eq!(locations(&offline), [loc(TAILSCALE, "/Volumes/naspi/papers/invoices")]);

    let dead = important(dir.path(), json!({ "under": "/Volumes/naspi/papers" }));
    assert_eq!(
        dead["miss"], "noVolumeAtPath",
        "a bare path never reaches a saved share"
    );
}

#[test]
fn folder_importance_reads_only_the_resolved_volumes_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    // Both stores hold `/papers`; only the Tailscale one scores it.
    write_db(
        dir.path(),
        TAILSCALE,
        &[("/papers", scored_signals(PathClass::UserContent, NOW))],
        NOW,
    );
    write_db(dir.path(), LAN, &[("/elsewhere", low_score_signals())], NOW);
    let explain = |volume_id: Option<&str>, path: &str| {
        let input = LocationInput {
            volume_id: volume_id.map(str::to_string),
            path: path.to_string(),
        };
        folder_importance_on(dir.path(), &facts(), &input, NOW).expect("answers")
    };

    let lan = explain(None, "/Volumes/naspi-1/papers");
    assert_eq!(
        lan["status"], "unscored",
        "the LAN store has no row, whatever another store holds"
    );
    assert_eq!(
        lan["location"],
        json!({ "volumeId": LAN, "path": "/Volumes/naspi-1/papers" })
    );

    let tailscale = explain(Some(TAILSCALE), "/Volumes/naspi/papers");
    assert_eq!(tailscale["status"], "scored");
    assert_eq!(
        tailscale["location"],
        json!({ "volumeId": TAILSCALE, "path": "/Volumes/naspi/papers" })
    );

    let floored = explain(Some(LAN), "/Volumes/naspi-1/papers/node_modules");
    assert_eq!(floored["status"], "floored");
    assert_eq!(floored["location"]["volumeId"], LAN);

    let dead = explain(None, "/Volumes/naspi/papers");
    assert_eq!(dead["miss"], "noVolumeAtPath");
}

#[test]
fn important_folders_rounds_scores_and_keeps_empty_fields_off_the_wire() {
    let folders = vec![
        RankedFolder {
            volume_id: "root".to_string(),
            path: "/Users/x/Projects".to_string(),
            score: 0.912_345,
            why: "projectRoot, changed today".to_string(),
            folded: 4,
            signals: cmdr_index::importance::FolderSignals::neutral(),
        },
        RankedFolder {
            volume_id: "root".to_string(),
            path: "/Users/x/Documents".to_string(),
            score: 0.7,
            why: String::new(),
            folded: 0,
            signals: cmdr_index::importance::FolderSignals::neutral(),
        },
    ];
    let overview = vec![VolumeOverview {
        volume_id: "root".to_string(),
        kind: Some("local"),
        generation: 5,
        folder_count: 1_200,
    }];
    let out = build_important_folders(folders, true, overview, &facts(), None);
    assert_eq!(out.folders[0].location.path, "/Users/x/Projects");
    assert_eq!(out.folders[0].score, 0.912, "rounded to three decimals");
    assert!(out.truncated);
    assert_eq!(out.volumes[0].generation, 5);

    let json = serde_json::to_value(&out).expect("serializes");
    assert_eq!(json["folders"][0]["why"], "projectRoot, changed today");
    assert_eq!(json["folders"][0]["folded"], 4);
    assert!(
        json["folders"][1].get("why").is_none() && json["folders"][1].get("folded").is_none(),
        "an empty why and a zero fold stay off the wire"
    );
    assert!(json.get("under").is_none());
}

#[test]
fn scored_result_flags_stale_when_score_predates_recompute() {
    let at = Location {
        volume_id: "root".to_string(),
        path: "/p".to_string(),
    };
    let FolderImportanceResult::Scored { stale, .. } = scored_result(at.clone(), 0.8, 3, 5, None) else {
        panic!("expected Scored");
    };
    assert!(stale, "as_of 3 < recompute 5 ⇒ stale");

    let FolderImportanceResult::Scored { stale, .. } = scored_result(at, 0.8, 5, 5, None) else {
        panic!("expected Scored");
    };
    assert!(!stale, "as_of == recompute ⇒ current");
}

#[test]
fn floored_and_unscored_carry_honest_reasons() {
    let at = |path: &str| Location {
        volume_id: "root".to_string(),
        path: path.to_string(),
    };
    let floored =
        serde_json::to_value(floored_result(at("/x/node_modules"), FloorReason::NameDenylisted)).expect("serializes");
    assert_eq!(floored["status"], "floored");
    assert_eq!(floored["reason"], "nameDenylisted");

    let unscored = serde_json::to_value(unscored_result(at("/x/new"))).expect("serializes");
    assert_eq!(unscored["status"], "unscored");
    assert!(unscored["note"].as_str().expect("note").contains("No stored weight"));
}
