//! F4 Replace lists: saving, order, the built-in's one-time seeding, and that they survive a restart.

use super::replace_presets::{
    MAX_PRESETS, PresetList, ReplacePreset, ReplacePresetRefusal, ReplacePresetStore, delete_in, save_in, seed_in,
};
use super::text_tools::ReplaceStep;

fn step(search: &str) -> ReplaceStep {
    ReplaceStep {
        fields: None,
        search: search.to_string(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        regex: false,
    }
}

fn preset(id: &str, name: &str) -> ReplacePreset {
    ReplacePreset {
        id: id.to_string(),
        name: name.to_string(),
        steps: vec![step("a")],
    }
}

fn names(presets: &[ReplacePreset]) -> Vec<&str> {
    presets.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn save_appends_and_a_taken_name_is_replaced_in_place() {
    let mut list = vec![preset("1", "One"), preset("2", "Two")];
    save_in(&mut list, preset("3", "Three"), MAX_PRESETS).expect("saves");
    assert_eq!(names(&list), ["One", "Two", "Three"]);
    let mut changed = preset("9", " one ");
    changed.steps = vec![step("x"), step("y")];
    save_in(&mut list, changed, MAX_PRESETS).expect("saves");
    assert_eq!(names(&list), ["one", "Two", "Three"], "the same name keeps its place");
    assert_eq!(list[0].id, "1");
    assert_eq!(list[0].steps.len(), 2);
}

#[test]
fn save_refuses_an_empty_list_and_a_full_store() {
    let mut list = vec![preset("1", "One")];
    let mut nameless = preset("2", "  ");
    assert_eq!(
        save_in(&mut list, nameless.clone(), 5),
        Err(ReplacePresetRefusal::Empty)
    );
    nameless.name = "Named".into();
    nameless.steps.clear();
    assert_eq!(save_in(&mut list, nameless, 5), Err(ReplacePresetRefusal::Empty));
    assert_eq!(
        save_in(&mut list, preset("2", "Two"), 1),
        Err(ReplacePresetRefusal::ListFull { max: 1 })
    );
    assert_eq!(names(&list), ["One"]);
}

#[test]
fn delete_removes_by_id() {
    let mut list = vec![preset("1", "One"), preset("2", "Two")];
    assert!(delete_in(&mut list, "1"));
    assert!(!delete_in(&mut list, "1"));
    assert_eq!(names(&list), ["Two"]);
}

#[test]
fn the_built_in_goes_in_once_and_a_deleted_one_stays_deleted() {
    let mut list = PresetList {
        entries: vec![preset("1", "Mine")],
        seed_version: 0,
    };
    assert!(seed_in(&mut list, MAX_PRESETS));
    assert_eq!(names(&list.entries), ["Mine", "Czech part words → numbers"]);
    assert_eq!(list.entries[1].steps.len(), 20);
    list.entries.pop();
    assert!(!seed_in(&mut list, MAX_PRESETS), "already at the current version");
    assert_eq!(names(&list.entries), ["Mine"]);
}

#[test]
fn a_user_list_with_the_built_ins_name_is_not_doubled() {
    let mut list = PresetList {
        entries: vec![preset("1", "czech part words → numbers")],
        seed_version: 0,
    };
    seed_in(&mut list, MAX_PRESETS);
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.seed_version, 1);
}

#[test]
fn lists_and_the_seed_version_survive_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tag-replace-presets.json");
    let store = ReplacePresetStore::new();
    store.load_at(&path);
    let after = store.edit_at(Some(&path), "a seed", |list| seed_in(list, MAX_PRESETS));
    assert_eq!(after.len(), 1);
    store.edit_at(Some(&path), "a save", |list| {
        save_in(&mut list.entries, preset("m", "Mine"), MAX_PRESETS).is_ok()
    });
    store.edit_at(Some(&path), "a delete", |list| {
        let id = list.entries[0].id.clone();
        delete_in(&mut list.entries, &id)
    });

    let reopened = ReplacePresetStore::new();
    reopened.load_at(&path);
    let after = reopened.edit_at(Some(&path), "a seed", |list| seed_in(list, MAX_PRESETS));
    assert_eq!(names(&after), ["Mine"], "the deleted built-in doesn't return");
}

#[test]
fn a_file_that_will_not_read_is_set_aside_and_not_reseeded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tag-replace-presets.json");
    std::fs::write(&path, "{not json").expect("write garbage");
    let notices = crate::state_recovery::testing::capture_recoveries();
    let store = ReplacePresetStore::new();
    store.load_at(&path);
    let after = store.edit_at(Some(&path), "a seed", |list| seed_in(list, MAX_PRESETS));
    assert!(after.is_empty());
    assert_eq!(crate::state_recovery::testing::kept_copies(&path).len(), 1);
    assert_eq!(notices.recoveries().len(), 1);
}
