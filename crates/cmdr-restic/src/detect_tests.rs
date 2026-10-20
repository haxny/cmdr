use super::is_restic_repo;
use cmdr_fs::entry::FileEntry;

fn file(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/repo/{name}"), false, false)
}

fn dir(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/repo/{name}"), true, false)
}

fn repo_listing() -> Vec<FileEntry> {
    vec![
        file("config"),
        dir("data"),
        dir("index"),
        dir("keys"),
        dir("locks"),
        dir("snapshots"),
    ]
}

#[test]
fn a_full_repo_layout_is_a_repo() {
    assert!(is_restic_repo(&repo_listing()));
}

#[test]
fn locks_is_optional() {
    let entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != "locks").collect();
    assert!(is_restic_repo(&entries));
}

#[test]
fn extra_entries_dont_disqualify_a_repo() {
    let mut entries = repo_listing();
    entries.push(file(".DS_Store"));
    entries.push(dir("notes"));
    assert!(is_restic_repo(&entries));
}

/// A repo root holds six entries. A crowded folder that happens to hold the five
/// names is answered on its length alone, so a 100k-entry listing costs one
/// comparison, never a scan.
#[test]
fn a_crowded_folder_isnt_a_repo() {
    let mut entries = repo_listing();
    entries.extend((0..super::MAX_REPO_ROOT_ENTRIES).map(|i| file(&format!("extra{i}"))));
    assert!(!is_restic_repo(&entries));
}

#[test]
fn each_required_entry_is_required() {
    for missing in ["config", "data", "index", "keys", "snapshots"] {
        let entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != missing).collect();
        assert!(!is_restic_repo(&entries), "a listing without `{missing}` isn't a repo");
    }
}

#[test]
fn a_lookalike_with_config_as_a_folder_isnt_a_repo() {
    // A project folder with a `config/` dir and a `data/` dir is common; the
    // `config` FILE is what a repo has.
    let mut entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != "config").collect();
    entries.push(dir("config"));
    assert!(!is_restic_repo(&entries));
}

#[test]
fn a_lookalike_with_data_as_a_file_isnt_a_repo() {
    let mut entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != "data").collect();
    entries.push(file("data"));
    assert!(!is_restic_repo(&entries));
}

#[test]
fn a_symlinked_config_doesnt_count() {
    // A symlink's target is unknown from the listing alone, and a repo restic
    // wrote never has one, so it isn't confirmed by a link.
    let mut entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != "config").collect();
    entries.push(FileEntry::new(
        "config".to_string(),
        "/repo/config".to_string(),
        false,
        true,
    ));
    assert!(!is_restic_repo(&entries));
}

#[test]
fn names_match_exactly() {
    let mut entries: Vec<_> = repo_listing().into_iter().filter(|e| e.name != "keys").collect();
    entries.push(dir("Keys"));
    assert!(!is_restic_repo(&entries));
}

#[test]
fn an_empty_folder_isnt_a_repo() {
    assert!(!is_restic_repo(&[]));
}
