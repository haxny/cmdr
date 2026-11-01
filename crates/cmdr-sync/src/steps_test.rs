use std::path::PathBuf;

use crate::test_tree::{Tree, caps, key};
use crate::*;

const T: u64 = 1_700_000_000;
const STAMP_DIR: &str = "2026-11-01 093000";

pub(crate) fn stamp() -> RunStamp {
    RunStamp::new(2026, 11, 1, 9, 30, 0)
}

/// A run between `/L` and `/R` with the given policy and volumes.
pub(crate) fn context(keep: KeepPolicy, left_caps: SyncCaps, right_caps: SyncCaps) -> RunContext {
    let (left, right) = (PathBuf::from("/L"), PathBuf::from("/R"));
    RunContext::new(left, right, left_caps, right_caps, keep, stamp(), None).unwrap()
}

pub(crate) fn no_trash() -> SyncCaps {
    SyncCaps {
        has_trash: false,
        ..caps()
    }
}

pub(crate) fn index_of(plan: &Plan, path: &str) -> u32 {
    let wanted = key(path);
    plan.items
        .iter()
        .find(|item| item.key == wanted)
        .unwrap_or_else(|| panic!("no item for {path}"))
        .index
}

/// Every row that does something.
pub(crate) fn acting(plan: &Plan) -> Vec<u32> {
    plan.items
        .iter()
        .filter(|item| item.action.target().is_some())
        .map(|item| item.index)
        .collect()
}

fn plan_of(mode: Mode, left: &Snapshot, right: &Snapshot) -> Plan {
    let options = PlanOptions {
        mode,
        rules: CompareRules::QUICK,
    };
    plan(left, right, None, &options)
}

fn versions_dir(root: &str, rel_parent: &str) -> PathBuf {
    let base = PathBuf::from(root).join(".cmdr-sync/versions").join(STAMP_DIR);
    if rel_parent.is_empty() {
        base
    } else {
        base.join(rel_parent)
    }
}

#[test]
fn steps_run_folders_keep_copy_remove_in_order() {
    let left = Tree::new()
        .file("a.txt", 5, T + 100)
        .file("n.txt", 1, T)
        .with_caps(no_trash())
        .build();
    let right = Tree::new()
        .file("a.txt", 4, T)
        .file("old.txt", 3, T)
        .with_caps(no_trash())
        .build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let ctx = context(KeepPolicy::default(), no_trash(), no_trash());
    let (a, n, old) = (
        index_of(&plan, "a.txt"),
        index_of(&plan, "n.txt"),
        index_of(&plan, "old.txt"),
    );
    let versions = KeepDestination::Folder {
        place: Place::Target,
        dir: versions_dir("/R", ""),
    };
    let got = steps(&plan, &acting(&plan), &ctx);
    assert!(got.refused.is_empty());
    assert_eq!(
        got.steps,
        vec![
            Step::MakeFolders {
                place: Place::Target,
                side: Side::Right,
                dirs: vec![versions_dir("/R", "")],
            },
            Step::KeepAside {
                side: Side::Right,
                items: vec![a],
                to: versions.clone(),
            },
            Step::Copy {
                from: Side::Left,
                dest_dir: PathBuf::from("/R"),
                items: vec![a, n],
            },
            Step::Remove {
                side: Side::Right,
                items: vec![old],
                to: versions,
            },
        ]
    );
}

#[test]
fn copies_group_by_direction_and_destination_folder() {
    let left = Tree::new()
        .file("d/x.txt", 1, T)
        .file("d/z.txt", 1, T)
        .file("y.txt", 1, T)
        .build();
    let right = Tree::new().file("d/w.txt", 1, T).file("v.txt", 1, T).build();
    let plan = plan_of(Mode::BothWays, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let at = |path| index_of(&plan, path);
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.steps,
        vec![
            Step::Copy {
                from: Side::Left,
                dest_dir: PathBuf::from("/R"),
                items: vec![at("y.txt")],
            },
            Step::Copy {
                from: Side::Left,
                dest_dir: PathBuf::from("/R/d"),
                items: vec![at("d/x.txt"), at("d/z.txt")],
            },
            Step::Copy {
                from: Side::Right,
                dest_dir: PathBuf::from("/L"),
                items: vec![at("v.txt")],
            },
            Step::Copy {
                from: Side::Right,
                dest_dir: PathBuf::from("/L/d"),
                items: vec![at("d/w.txt")],
            },
        ]
    );
}

#[test]
fn expanded_one_sided_folder_creates_its_target_folders_first() {
    let left = Tree::new()
        .file("k.txt", 1, T)
        .file("docs/a.txt", 1, T)
        .file("docs/sub/b.txt", 1, T)
        .held_back("docs", 1)
        .build();
    let right = Tree::new().file("k.txt", 1, T).build();
    let plan = plan_of(Mode::Update, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.steps,
        vec![
            Step::MakeFolders {
                place: Place::Target,
                side: Side::Right,
                dirs: vec![PathBuf::from("/R/docs")],
            },
            Step::Copy {
                from: Side::Left,
                dest_dir: PathBuf::from("/R/docs"),
                items: vec![index_of(&plan, "docs/a.txt"), index_of(&plan, "docs/sub")],
            },
        ]
    );
}

#[test]
fn nested_split_folders_create_every_missing_level() {
    let left = Tree::new()
        .file("k.txt", 1, T)
        .file("a/b/c.txt", 1, T)
        .held_back("a", 1)
        .held_back("a/b", 1)
        .build();
    let right = Tree::new().file("k.txt", 1, T).build();
    let plan = plan_of(Mode::Update, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.steps[0],
        Step::MakeFolders {
            place: Place::Target,
            side: Side::Right,
            dirs: vec![PathBuf::from("/R/a"), PathBuf::from("/R/a/b")],
        }
    );
}

#[test]
fn copies_land_in_the_target_sides_own_spelling_of_the_folder() {
    let left = Tree::new().file("docs/new.txt", 1, T).build();
    let mut right = Tree::new().folder("docs").build();
    right.entries.get_mut(&key("docs")).unwrap().rel_path = "Docs".to_owned();
    let plan = plan_of(Mode::Update, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.steps,
        vec![Step::Copy {
            from: Side::Left,
            dest_dir: PathBuf::from("/R/Docs"),
            items: vec![index_of(&plan, "docs/new.txt")],
        }]
    );
}

#[test]
fn trash_only_without_a_trash_refuses_replacements_and_removals() {
    let left = Tree::new().file("a.txt", 5, T + 100).file("n.txt", 1, T).build();
    let right = Tree::new().file("a.txt", 4, T).file("old.txt", 3, T).build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let keep = KeepPolicy {
        place: KeepPlace::TrashOnly,
        ..KeepPolicy::default()
    };
    let ctx = context(keep, caps(), no_trash());
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.refused,
        vec![
            (index_of(&plan, "a.txt"), KeepRefusal::NoTrash),
            (index_of(&plan, "old.txt"), KeepRefusal::NoTrash),
        ]
    );
    assert_eq!(
        got.steps,
        vec![Step::Copy {
            from: Side::Left,
            dest_dir: PathBuf::from("/R"),
            items: vec![index_of(&plan, "n.txt")],
        }]
    );
}

#[test]
fn replacements_stage_on_the_target_volume_even_with_a_trash() {
    let left = Tree::new().file("a.txt", 5, T + 100).build();
    let right = Tree::new().file("a.txt", 4, T).file("old.txt", 3, T).build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    let staged = KeepDestination::Folder {
        place: Place::Target,
        dir: versions_dir("/R", ""),
    };
    assert_eq!(
        got.steps,
        vec![
            Step::MakeFolders {
                place: Place::Target,
                side: Side::Right,
                dirs: vec![versions_dir("/R", "")],
            },
            Step::KeepAside {
                side: Side::Right,
                items: vec![index_of(&plan, "a.txt")],
                to: staged,
            },
            Step::Copy {
                from: Side::Left,
                dest_dir: PathBuf::from("/R"),
                items: vec![index_of(&plan, "a.txt")],
            },
            Step::Remove {
                side: Side::Right,
                items: vec![index_of(&plan, "old.txt")],
                to: KeepDestination::Trash,
            },
        ]
    );
}

#[test]
fn each_copy_group_sets_aside_right_before_it_copies() {
    let left = Tree::new()
        .file("a.txt", 5, T + 100)
        .file("d/b.txt", 5, T + 100)
        .build();
    let right = Tree::new().file("a.txt", 4, T).file("d/b.txt", 4, T).build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let ctx = context(KeepPolicy::default(), no_trash(), no_trash());
    let got = steps(&plan, &acting(&plan), &ctx);
    let kinds: Vec<&str> = got
        .steps
        .iter()
        .map(|step| match step {
            Step::MakeFolders { .. } => "make",
            Step::KeepAside { .. } => "keep",
            Step::Copy { .. } => "copy",
            Step::Remove { .. } => "remove",
        })
        .collect();
    assert_eq!(kinds, ["make", "keep", "copy", "keep", "copy"]);
}

#[test]
fn a_custom_folder_inside_a_sync_root_is_refused() {
    let keep = KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: "/R/kept".to_owned(),
        },
    };
    let (left, right) = (PathBuf::from("/L"), PathBuf::from("/R"));
    let refused = RunContext::new(left, right, caps(), caps(), keep, stamp(), None);
    assert_eq!(refused, Err(KeepRefusal::InsideSyncFolder));
}

#[test]
fn replacements_under_a_custom_folder_stage_on_the_target_and_end_on_the_local_disk() {
    let left = Tree::new().file("d/a.txt", 5, T + 100).build();
    let right = Tree::new().file("d/a.txt", 4, T).build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let keep = KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: "/Kept".to_owned(),
        },
    };
    let ctx = context(keep, caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    assert_eq!(
        got.steps[..2],
        [
            Step::MakeFolders {
                place: Place::Target,
                side: Side::Right,
                dirs: vec![versions_dir("/R", "d")],
            },
            Step::MakeFolders {
                place: Place::LocalDisk,
                side: Side::Right,
                dirs: vec![PathBuf::from("/Kept").join(STAMP_DIR).join("d")],
            },
        ]
    );
}

#[test]
fn only_rows_in_the_run_get_steps() {
    let left = Tree::new().file("a.txt", 1, T).file("b.txt", 1, T).build();
    let right = Tree::new().build();
    let plan = plan_of(Mode::Update, &left, &right);
    let ctx = context(KeepPolicy::default(), caps(), caps());
    let b = index_of(&plan, "b.txt");
    let got = steps(&plan, &[b, 999], &ctx);
    assert_eq!(
        got.steps,
        vec![Step::Copy {
            from: Side::Left,
            dest_dir: PathBuf::from("/R"),
            items: vec![b],
        }]
    );
}

#[test]
fn custom_versions_folder_lives_on_the_local_disk() {
    let left = Tree::new().folder("d").build();
    let right = Tree::new().file("k.txt", 1, T).file("d/old.txt", 3, T).build();
    let plan = plan_of(Mode::Mirror, &left, &right);
    let keep = KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: "/Kept".to_owned(),
        },
    };
    let ctx = context(keep, caps(), caps());
    let got = steps(&plan, &acting(&plan), &ctx);
    let local = PathBuf::from("/Kept").join(STAMP_DIR);
    assert_eq!(
        got.steps,
        vec![
            Step::MakeFolders {
                place: Place::LocalDisk,
                side: Side::Right,
                dirs: vec![local.clone(), local.join("d")],
            },
            Step::Remove {
                side: Side::Right,
                items: vec![index_of(&plan, "k.txt")],
                to: KeepDestination::Folder {
                    place: Place::LocalDisk,
                    dir: local.clone(),
                },
            },
            Step::Remove {
                side: Side::Right,
                items: vec![index_of(&plan, "d/old.txt")],
                to: KeepDestination::Folder {
                    place: Place::LocalDisk,
                    dir: local.join("d"),
                },
            },
        ]
    );
}
