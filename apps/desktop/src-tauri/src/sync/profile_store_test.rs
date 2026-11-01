//! Profiles: order, replace-in-place, greyed invalid entries, unknown fields, a damaged
//! file, and the newest-fifty memory of options per pair of roots.

use serde_json::{Value, json};

use cmdr_sync::{ProfileOverrides, ProfileProblem, RootRef, ShowFilters, SyncOptions, SyncProfile};

use super::profile_store::{
    BadRoots, MAX_ENTRY_BYTES, MAX_PROFILES, MAX_REMEMBERED_PAIRS, NameTaken, ProfileEntry, ProfileMove,
    ProfileSaveRefusal, ProfileStore, WriteFailed, delete_in, entries_of, last_options_in, move_in,
    remember_options_in, rename_in, save_in,
};

fn root(path: &str) -> RootRef {
    RootRef {
        volume_id: "root".to_owned(),
        path: path.to_owned(),
    }
}

fn profile(id: &str, name: &str) -> SyncProfile {
    SyncProfile {
        id: id.to_owned(),
        name: name.to_owned(),
        left: root(&format!("/left-{id}")),
        right: root(&format!("/right-{id}")),
        options: SyncOptions::default(),
        show_filters: ShowFilters::default(),
        restore_show_filters: false,
        overrides: ProfileOverrides::default(),
    }
}

fn names(list: &super::profile_store::ProfileList) -> Vec<String> {
    entries_of(list)
        .into_iter()
        .map(|entry| match entry {
            ProfileEntry::Valid { profile } => profile.name,
            ProfileEntry::Invalid { name, .. } => name,
        })
        .collect()
}

#[test]
fn save_appends_and_same_name_replaces_in_place() {
    let mut list = super::profile_store::ProfileList::default();
    save_in(&mut list, profile("a", "Photos")).unwrap();
    save_in(&mut list, profile("b", "Music")).unwrap();
    save_in(&mut list, profile("c", "Docs")).unwrap();
    let mut again = profile("z", "  PHOTOS ");
    again.options.subfolders = false;
    save_in(&mut list, again).unwrap();
    assert_eq!(names(&list), ["  PHOTOS ", "Music", "Docs"]);
    let ProfileEntry::Valid { profile: first } = &entries_of(&list)[0] else {
        panic!("valid");
    };
    assert!(!first.options.subfolders, "the replacing profile's settings win");
}

#[test]
fn a_full_list_refuses_a_new_name_but_still_replaces() {
    let mut list = super::profile_store::ProfileList::default();
    for i in 0..MAX_PROFILES {
        save_in(&mut list, profile(&format!("id{i}"), &format!("P{i}"))).unwrap();
    }
    assert_eq!(
        save_in(&mut list, profile("new", "One more")),
        Err(ProfileSaveRefusal::ListFull {
            max: u32::try_from(MAX_PROFILES).unwrap()
        })
    );
    assert!(save_in(&mut list, profile("x", "p0")).is_ok());
}

#[test]
fn an_invalid_profile_is_refused_on_save() {
    let mut list = super::profile_store::ProfileList::default();
    let refusal = save_in(&mut list, profile("a", "   ")).unwrap_err();
    assert_eq!(
        refusal,
        ProfileSaveRefusal::Invalid {
            problem: ProfileProblem::EmptyName
        }
    );
    assert!(entries_of(&list).is_empty());
}

#[test]
fn invalid_profile_is_listed_greyed_not_dropped() {
    let mut list = super::profile_store::ProfileList::default();
    save_in(&mut list, profile("a", "Photos")).unwrap();
    // Stored by hand: a mask the parser refuses, and an entry no build can read.
    let mut bad_mask = serde_json::to_value(profile("b", "Broken masks")).unwrap();
    bad_mask["options"]["masks"] = json!("a | b | c");
    list.entries.push(bad_mask);
    list.entries
        .push(json!({ "id": "c", "name": "From the future", "left": 7 }));
    let entries = entries_of(&list);
    assert_eq!(entries.len(), 3);
    assert!(matches!(
        &entries[1],
        ProfileEntry::Invalid { id, problem: ProfileProblem::BadMask { .. }, .. } if id == "b"
    ));
    assert!(matches!(
        &entries[2],
        ProfileEntry::Invalid { name, problem: ProfileProblem::Unreadable, .. } if name == "From the future"
    ));
    assert_eq!(list.entries.len(), 3, "nothing is dropped from the stored list");
}

#[test]
fn unknown_fields_survive_a_save() {
    let mut list = super::profile_store::ProfileList::default();
    let mut stored = serde_json::to_value(profile("a", "Photos")).unwrap();
    stored["fromANewerBuild"] = json!({ "keep": true });
    stored["options"]["futureOption"] = json!(3);
    list.entries.push(stored);
    let mut edited = profile("a", "Photos");
    edited.options.subfolders = false;
    save_in(&mut list, edited).unwrap();
    let Value::Object(object) = &list.entries[0] else {
        panic!("object");
    };
    assert_eq!(object["fromANewerBuild"], json!({ "keep": true }));
    assert_eq!(object["options"]["futureOption"], json!(3));
    assert_eq!(object["options"]["subfolders"], json!(false));
}

#[test]
fn switching_the_keep_location_leaves_no_stale_variant() {
    use cmdr_sync::{KeepPlace, KeepPolicy, VersionsLocation};
    let mut list = super::profile_store::ProfileList::default();
    let mut first = profile("a", "Photos");
    first.overrides.keep = Some(KeepPolicy::default());
    save_in(&mut list, first).unwrap();
    let mut second = profile("a", "Photos");
    second.overrides.keep = Some(KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::Custom {
            template: "/Backups/{profile}".to_owned(),
        },
    });
    save_in(&mut list, second.clone()).unwrap();
    let ProfileEntry::Valid { profile: read } = &entries_of(&list)[0] else {
        panic!("the saved profile reads back");
    };
    assert_eq!(**read, second);
}

#[test]
fn rename_delete_and_move_keep_the_order() {
    let mut list = super::profile_store::ProfileList::default();
    for (id, name) in [("a", "Photos"), ("b", "Music"), ("c", "Docs")] {
        save_in(&mut list, profile(id, name)).unwrap();
    }
    assert_eq!(rename_in(&mut list, "b", "  Songs "), Ok(true));
    assert_eq!(names(&list), ["Photos", "Songs", "Docs"]);
    assert_eq!(
        rename_in(&mut list, "c", "photos"),
        Err(NameTaken),
        "a taken name refuses; it never deletes the other profile"
    );
    assert_eq!(names(&list), ["Photos", "Songs", "Docs"]);
    assert_eq!(
        rename_in(&mut list, "a", "PHOTOS"),
        Ok(true),
        "a profile may change its own name's case"
    );
    assert_eq!(rename_in(&mut list, "b", "   "), Ok(false));
    assert!(move_in(&mut list, "c", ProfileMove::Up));
    assert_eq!(names(&list), ["PHOTOS", "Docs", "Songs"]);
    assert!(move_in(&mut list, "c", ProfileMove::Up));
    assert!(!move_in(&mut list, "c", ProfileMove::Up), "no move at the top");
    assert!(delete_in(&mut list, "c"));
    assert!(!delete_in(&mut list, "c"));
    assert_eq!(names(&list), ["PHOTOS", "Songs"]);
}

#[test]
fn profiles_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync-profiles.json");
    let writer = ProfileStore::new();
    writer
        .edit_at(Some(&path), "a save", |list| {
            save_in(list, profile("a", "Photos")).is_ok()
        })
        .unwrap();
    let reader = ProfileStore::new();
    reader.load_at(&path);
    assert_eq!(reader.entries().len(), 1);
}

#[test]
fn unreadable_file_is_set_aside_never_saved_over() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync-profiles.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let store = ProfileStore::new();
    store.load_at(&path);
    assert!(store.entries().is_empty(), "starts empty");
    assert!(!path.exists(), "the damaged file was moved away");
    let aside: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(aside.len(), 1, "one set-aside copy: {aside:?}");
    assert_eq!(std::fs::read(dir.path().join(&aside[0])).unwrap(), b"{ not json");
}

#[test]
fn a_file_from_another_schema_is_set_aside_too() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync-profiles.json");
    std::fs::write(&path, br#"{"_schemaVersion": 99, "entries": []}"#).unwrap();
    let store = ProfileStore::new();
    store.load_at(&path);
    assert!(!path.exists());
}

#[test]
fn last_options_keep_the_newest_fifty_pairs() {
    let mut list = super::profile_store::ProfileList::default();
    for i in 0..(MAX_REMEMBERED_PAIRS + 5) {
        let options = SyncOptions {
            masks: format!("*.{i}"),
            ..SyncOptions::default()
        };
        remember_options_in(&mut list, &root(&format!("/l{i}")), &root(&format!("/r{i}")), &options).unwrap();
    }
    assert_eq!(list.last_options.len(), MAX_REMEMBERED_PAIRS);
    assert_eq!(
        last_options_in(&list, &root("/l0"), &root("/r0")),
        None,
        "the oldest went"
    );
    let newest = last_options_in(&list, &root("/l54"), &root("/r54")).unwrap();
    assert_eq!(newest.masks, "*.54");
    // The pair is ordered, and saving it again replaces and refreshes it.
    assert_eq!(last_options_in(&list, &root("/r54"), &root("/l54")), None);
    remember_options_in(&mut list, &root("/l5"), &root("/r5"), &SyncOptions::default()).unwrap();
    assert_eq!(list.last_options.len(), MAX_REMEMBERED_PAIRS);
    assert_eq!(last_options_in(&list, &root("/l5"), &root("/r5")).unwrap().masks, "");
}

#[test]
fn last_options_reach_disk_and_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync-profiles.json");
    let writer = ProfileStore::new();
    writer
        .edit_at(Some(&path), "remembering options", |list| {
            remember_options_in(list, &root("/l"), &root("/r"), &SyncOptions::default()).unwrap()
        })
        .unwrap();
    let reader = ProfileStore::new();
    reader.load_at(&path);
    assert_eq!(
        reader.last_options(&root("/l"), &root("/r")),
        Some(SyncOptions::default())
    );
}

#[test]
fn an_entry_without_an_id_is_listed_greyed_and_can_be_deleted() {
    let mut list = super::profile_store::ProfileList::default();
    list.entries.push(json!({ "name": "Orphan" }));
    save_in(&mut list, profile("a", "Photos")).unwrap();
    let entries = entries_of(&list);
    let ProfileEntry::Invalid { id, name, problem } = &entries[0] else {
        panic!("greyed");
    };
    assert_eq!((name.as_str(), *problem), ("Orphan", ProfileProblem::Unreadable));
    assert!(delete_in(&mut list, id));
    assert_eq!(names(&list), ["Photos"]);
}

#[test]
fn a_write_that_does_not_land_is_reported_not_swallowed() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let path = blocker.join("sync-profiles.json");
    let store = ProfileStore::new();
    let result = store.edit_at(Some(&path), "a save", |list| {
        save_in(list, profile("a", "Photos")).is_ok()
    });
    assert_eq!(result, Err(WriteFailed));
    assert_eq!(store.entries().len(), 1, "the change stays in memory");
}

#[test]
fn concurrent_edits_never_lose_the_newer_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync-profiles.json");
    let store = std::sync::Arc::new(ProfileStore::new());
    let threads: Vec<_> = (0..8)
        .map(|t| {
            let (store, path) = (store.clone(), path.clone());
            std::thread::spawn(move || {
                for i in 0..10 {
                    let name = format!("P{t}-{i}");
                    store
                        .edit_at(Some(&path), "a save", |list| {
                            save_in(list, profile(&name, &name)).is_ok()
                        })
                        .unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(store.entries().len(), 80);
    let reader = ProfileStore::new();
    reader.load_at(&path);
    assert_eq!(reader.entries(), store.entries(), "the file holds the last change");
}

#[test]
fn options_for_roots_that_arent_folder_paths_are_refused() {
    let mut list = super::profile_store::ProfileList::default();
    let options = SyncOptions::default();
    let bad = [
        root("relative/path"),
        root("/a/../b"),
        root(""),
        root(&format!("/{}", "x".repeat(2000))),
        RootRef {
            volume_id: String::new(),
            path: "/l".to_owned(),
        },
    ];
    for left in &bad {
        assert_eq!(
            remember_options_in(&mut list, left, &root("/r"), &options),
            Err(BadRoots)
        );
    }
    let huge = SyncOptions {
        masks: "*".repeat(MAX_ENTRY_BYTES),
        ..SyncOptions::default()
    };
    assert_eq!(
        remember_options_in(&mut list, &root("/l"), &root("/r"), &huge),
        Err(BadRoots)
    );
    assert!(list.last_options.is_empty());
    // A phone's roots are URLs, and are remembered.
    let phone = RootRef {
        volume_id: "mtp-0-1".to_owned(),
        path: "mtp://mtp-0-1/65537/DCIM".to_owned(),
    };
    assert_eq!(remember_options_in(&mut list, &phone, &root("/r"), &options), Ok(true));
    let adb = RootRef {
        volume_id: "adb-R5CT".to_owned(),
        path: "adb://R5CT/sdcard".to_owned(),
    };
    assert_eq!(remember_options_in(&mut list, &adb, &root("/r"), &options), Ok(true));
}

#[test]
fn a_profile_past_the_size_cap_is_refused_and_the_list_stays() {
    let mut list = super::profile_store::ProfileList::default();
    save_in(&mut list, profile("a", "A")).unwrap();
    let huge = SyncProfile {
        options: SyncOptions {
            excludes: vec!["x".repeat(MAX_ENTRY_BYTES)],
            ..SyncOptions::default()
        },
        ..profile("b", "B")
    };
    assert!(matches!(
        save_in(&mut list, huge),
        Err(ProfileSaveRefusal::TooLarge { .. })
    ));
    assert_eq!(names(&list), vec!["A".to_owned()]);
}

#[test]
fn an_entry_without_an_id_never_shares_a_key_with_a_real_one() {
    // A stored id that looks like the synthetic key of another entry.
    let list = super::profile_store::ProfileList {
        entries: vec![json!({ "name": "No id" }), json!({ "id": "no id 0", "name": "Sneaky" })],
        last_options: Vec::new(),
    };
    let ids: Vec<String> = entries_of(&list)
        .into_iter()
        .map(|entry| match entry {
            ProfileEntry::Valid { profile } => profile.id,
            ProfileEntry::Invalid { id, .. } => id,
        })
        .collect();
    assert_ne!(ids[0], ids[1]);
    let mut deleting = list.clone();
    assert!(delete_in(&mut deleting, &ids[0]));
    assert_eq!(names(&deleting), vec!["Sneaky".to_owned()]);
}
