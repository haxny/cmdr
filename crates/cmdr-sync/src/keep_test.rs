use std::path::{Component, Path, PathBuf};

use proptest::prelude::*;

use crate::test_tree::caps;
use crate::*;

fn stamp() -> RunStamp {
    RunStamp::new(2026, 11, 1, 9, 30, 0)
}

fn policy(place: KeepPlace) -> KeepPolicy {
    KeepPolicy {
        place,
        ..KeepPolicy::default()
    }
}

fn custom(template: &str) -> KeepPolicy {
    KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: template.to_owned(),
        },
    }
}

fn root_of(policy: &KeepPolicy) -> VersionsRoot {
    versions_root(policy, Path::new("/r"), None, &stamp(), 0).unwrap()
}

fn caps_with_trash(has_trash: bool) -> SyncCaps {
    SyncCaps { has_trash, ..caps() }
}

#[test]
fn stamp_reads_as_date_and_time() {
    assert_eq!(stamp().as_str(), "2026-11-01 093000");
    assert_eq!(RunStamp::new(2027, 1, 2, 3, 4, 5).as_str(), "2027-01-02 030405");
}

#[test]
fn default_policy_is_trash_else_a_versions_folder_in_the_target_root() {
    let default = KeepPolicy::default();
    assert_eq!(default.place, KeepPlace::TrashElseVersions);
    assert_eq!(
        default.location,
        VersionsLocation::TargetRoot {
            folder_name: DEFAULT_VERSIONS_FOLDER.to_owned()
        }
    );
    assert_eq!(DEFAULT_VERSIONS_FOLDER, ".cmdr-sync/versions");
}

#[test]
fn trash_else_versions_picks_trash_only_with_a_trash() {
    let policy = policy(KeepPlace::TrashElseVersions);
    let versions = root_of(&policy);
    let with = keep_destination(&policy, &caps_with_trash(true), &versions, "a/b.txt");
    assert_eq!(with, KeepDestination::Trash);
    let without = keep_destination(&policy, &caps_with_trash(false), &versions, "a/b.txt");
    assert_eq!(
        without,
        KeepDestination::Folder {
            place: Place::Target,
            dir: PathBuf::from("/r/.cmdr-sync/versions/2026-11-01 093000/a"),
        }
    );
}

#[test]
fn versions_always_ignores_the_trash() {
    let policy = policy(KeepPlace::VersionsAlways);
    let versions = root_of(&policy);
    let destination = keep_destination(&policy, &caps_with_trash(true), &versions, "b.txt");
    assert_eq!(
        destination,
        KeepDestination::Folder {
            place: Place::Target,
            dir: PathBuf::from("/r/.cmdr-sync/versions/2026-11-01 093000"),
        }
    );
}

#[test]
fn trash_only_without_trash_is_refused_never_deleted() {
    let policy = policy(KeepPlace::TrashOnly);
    let versions = root_of(&policy);
    let d = keep_destination(&policy, &caps_with_trash(false), &versions, "a/b.txt");
    assert!(matches!(d, KeepDestination::Refused(KeepRefusal::NoTrash)));
    let with = keep_destination(&policy, &caps_with_trash(true), &versions, "a/b.txt");
    assert_eq!(with, KeepDestination::Trash);
}

#[test]
fn default_versions_path_is_dated_under_the_target_root() {
    // root /r, rel "a/b.txt", stamp 2026-11-01 093000 → /r/.cmdr-sync/versions/2026-11-01 093000/a
    let policy = policy(KeepPlace::VersionsAlways);
    let versions = root_of(&policy);
    assert_eq!(versions.place, Place::Target);
    assert_eq!(versions.dir, PathBuf::from("/r/.cmdr-sync/versions/2026-11-01 093000"));
    let destination = keep_destination(&policy, &caps(), &versions, "a/b.txt");
    assert_eq!(
        destination,
        KeepDestination::Folder {
            place: Place::Target,
            dir: PathBuf::from("/r/.cmdr-sync/versions/2026-11-01 093000/a"),
        }
    );
}

#[test]
fn custom_template_expands_root_profile_and_date_on_the_local_disk() {
    let policy = custom("/Users/me/Kept/{profile}/{root}/{date}");
    let versions = versions_root(&policy, Path::new("/Volumes/Backup"), Some("Photos"), &stamp(), 0).unwrap();
    assert_eq!(versions.place, Place::LocalDisk);
    assert_eq!(
        versions.dir,
        PathBuf::from("/Users/me/Kept/Photos/Volumes/Backup/2026-11-01 093000")
    );
    let unnamed = versions_root(&policy, Path::new("/Volumes/Backup"), None, &stamp(), 0).unwrap();
    assert_eq!(
        unnamed.dir,
        PathBuf::from("/Users/me/Kept/No profile/Volumes/Backup/2026-11-01 093000")
    );
}

#[test]
fn custom_template_without_date_gets_one() {
    let policy = custom("/Users/me/Kept");
    let versions = versions_root(&policy, Path::new("/r"), None, &stamp(), 0).unwrap();
    assert_eq!(versions.dir, PathBuf::from("/Users/me/Kept/2026-11-01 093000"));
}

#[test]
fn custom_template_must_be_an_absolute_path_with_known_placeholders() {
    for template in ["", "relative/{date}", "/a/{nope}", "/a/{date", "/a/date}"] {
        let refused = versions_root(&custom(template), Path::new("/r"), None, &stamp(), 0);
        assert_eq!(refused, Err(KeepRefusal::BadTemplate), "{template:?}");
    }
}

#[test]
fn placeholders_cannot_climb_out() {
    let up = versions_root(&custom("/k/{profile}"), Path::new("/r"), Some(".."), &stamp(), 0);
    assert_eq!(up, Err(KeepRefusal::EscapesFolder));
    let literal = versions_root(&custom("/k/../etc"), Path::new("/r"), None, &stamp(), 0);
    assert_eq!(literal, Err(KeepRefusal::EscapesFolder));
    let root_up = versions_root(&custom("/k/{root}"), Path::new("/a/../b"), None, &stamp(), 0);
    assert_eq!(root_up, Err(KeepRefusal::EscapesFolder));
    // A slash in a profile name stays inside one folder name.
    let slashed = versions_root(&custom("/k/{profile}"), Path::new("/r"), Some("a/b"), &stamp(), 0).unwrap();
    assert_eq!(slashed.dir, PathBuf::from("/k/a-b/2026-11-01 093000"));
}

#[test]
fn folder_name_must_stay_inside_the_target_root() {
    for folder_name in ["", "/abs", "../out", "a/../../b"] {
        let policy = KeepPolicy {
            place: KeepPlace::VersionsAlways,
            location: VersionsLocation::TargetRoot {
                folder_name: folder_name.to_owned(),
            },
        };
        let refused = versions_root(&policy, Path::new("/r"), None, &stamp(), 0);
        assert_eq!(refused, Err(KeepRefusal::EscapesFolder), "{folder_name:?}");
    }
}

#[test]
fn second_attempt_never_reuses_the_folder() {
    let policy = policy(KeepPlace::VersionsAlways);
    let first = versions_root(&policy, Path::new("/r"), None, &stamp(), 0).unwrap();
    let second = versions_root(&policy, Path::new("/r"), None, &stamp(), 1).unwrap();
    let third = versions_root(&policy, Path::new("/r"), None, &stamp(), 2).unwrap();
    assert_eq!(
        second.dir,
        PathBuf::from("/r/.cmdr-sync/versions/2026-11-01 093000 (2)")
    );
    assert_ne!(first.dir, second.dir);
    assert_ne!(second.dir, third.dir);
    let dated = versions_root(&custom("/k/{date}/x"), Path::new("/r"), None, &stamp(), 1).unwrap();
    assert_eq!(dated.dir, PathBuf::from("/k/2026-11-01 093000 (2)/x"));
}

#[test]
fn a_relative_path_that_climbs_is_refused() {
    let policy = policy(KeepPlace::VersionsAlways);
    let versions = root_of(&policy);
    for rel in ["../x", "a/../../x", "/abs", "a//b", "", "./x"] {
        let destination = keep_destination(&policy, &caps(), &versions, rel);
        assert_eq!(
            destination,
            KeepDestination::Refused(KeepRefusal::EscapesFolder),
            "{rel:?}"
        );
    }
}

fn arb_component() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["a", "B c", "é", "..", ".", "", "x.txt", "{date}", "/"]).prop_map(str::to_owned)
}

fn arb_rel() -> impl Strategy<Value = String> {
    prop::collection::vec(arb_component(), 1..5).prop_map(|parts| parts.join("/"))
}

fn arb_policy() -> impl Strategy<Value = KeepPolicy> {
    let place = prop::sample::select(vec![
        KeepPlace::TrashElseVersions,
        KeepPlace::VersionsAlways,
        KeepPlace::TrashOnly,
    ]);
    let location = prop_oneof![
        arb_rel().prop_map(|folder_name| VersionsLocation::TargetRoot { folder_name }),
        arb_rel().prop_map(|rest| VersionsLocation::Custom {
            template: format!("/k/{rest}")
        }),
    ];
    (place, location).prop_map(|(place, location)| KeepPolicy { place, location })
}

fn climbs(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

proptest! {
    #[test]
    fn kept_paths_never_escape_the_versions_folder(
        policy in arb_policy(),
        root in arb_rel(),
        profile in proptest::option::of(arb_rel()),
        rel in arb_rel(),
        attempt in 0u32..3,
        has_trash in any::<bool>(),
    ) {
        let target_root = PathBuf::from(format!("/{root}"));
        let Ok(versions) = versions_root(&policy, &target_root, profile.as_deref(), &stamp(), attempt) else {
            return Ok(());
        };
        prop_assert!(versions.dir.is_absolute());
        prop_assert!(!climbs(&versions.dir), "{:?}", versions.dir);
        if let VersionsLocation::TargetRoot { .. } = policy.location {
            prop_assert!(versions.dir.starts_with(&target_root));
        }
        let next = versions_root(&policy, &target_root, profile.as_deref(), &stamp(), attempt + 1).unwrap();
        prop_assert_ne!(&next.dir, &versions.dir);
        match keep_destination(&policy, &caps_with_trash(has_trash), &versions, &rel) {
            KeepDestination::Folder { place, dir } => {
                prop_assert_eq!(place, versions.place);
                prop_assert!(dir.starts_with(&versions.dir));
                prop_assert!(!climbs(&dir));
                let depth = dir.components().count() - versions.dir.components().count();
                prop_assert_eq!(depth + 1, rel.split('/').count());
            }
            KeepDestination::Trash => prop_assert!(has_trash),
            KeepDestination::Refused(KeepRefusal::NoTrash) => {
                prop_assert!(!has_trash);
                prop_assert_eq!(policy.place, KeepPlace::TrashOnly);
            }
            KeepDestination::Refused(_) => {
                prop_assert!(rel.split('/').any(|part| part.is_empty() || part == "." || part == ".."));
            }
        }
    }
}
