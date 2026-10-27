//! A sealed undo record writes a file back only while it is the file the apply left: never
//! into another file that took its name, never over a later write.

use std::fs;
use std::path::{Path, PathBuf};

use lofty::ogg::tag::VorbisComments;

use crate::fingerprint::Fingerprint;
use crate::model::{FileTags, TagField};
use crate::test_files::write_flac;
use crate::undo::{ApplyId, RestoreReport, UndoLimits, UndoStore};
use crate::write_test::{MUSIC, read, strings, write};

const ROOMY: UndoLimits = UndoLimits {
    max_applies: 50,
    max_bytes: u64::MAX,
};

fn id() -> ApplyId {
    ApplyId("op-1".to_owned())
}

fn titled(title: &str) -> FileTags {
    FileTags {
        fields: [(TagField::Title, strings(&[title]))].into(),
        ..FileTags::default()
    }
}

fn title(path: &Path) -> Vec<String> {
    read(path).fields[&TagField::Title].clone()
}

/// A FLAC titled `title` in `dir`.
fn flac(dir: &Path, name: &str, title: &str) -> PathBuf {
    let path = write_flac(dir, name, VorbisComments::new(), 0);
    write(&path, &titled(title), &MUSIC);
    path
}

/// Records `paths`, writes each a new title (`<old> new`), and seals the record.
fn apply(store: &UndoStore, paths: &[PathBuf]) {
    store.record(&id(), paths).unwrap();
    let written: Vec<(PathBuf, Fingerprint)> = paths
        .iter()
        .map(|path| {
            let new = format!("{} new", title(path)[0]);
            write(path, &titled(&new), &MUSIC);
            (path.clone(), Fingerprint::of(path).expect("stats"))
        })
        .collect();
    store.seal(&id(), &written).unwrap();
}

fn swap(a: &Path, b: &Path) {
    let tmp = a.with_extension("swap");
    fs::rename(a, &tmp).unwrap();
    fs::rename(b, a).unwrap();
    fs::rename(&tmp, b).unwrap();
}

#[test]
fn a_sealed_file_still_as_written_is_restored() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "a.flac", "A");
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    apply(&store, std::slice::from_ref(&a));

    let report = store.restore(&id(), &MUSIC).unwrap();

    assert_eq!(
        report,
        RestoreReport {
            restored: vec![a.clone()],
            ..RestoreReport::default()
        }
    );
    assert_eq!(title(&a), strings(&["A"]));
}

/// Ruling 20: a rename after the write swapped two names, so each path now holds the other
/// file. Neither gets the other's old tags; both are reported as changed.
#[test]
fn files_that_swapped_names_after_the_write_are_left_alone() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "01.flac", "A");
    let b = flac(files.path(), "03.flac", "B");
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    apply(&store, &[a.clone(), b.clone()]);
    swap(&a, &b);

    let report = store.restore(&id(), &MUSIC).unwrap();

    assert_eq!(
        report,
        RestoreReport {
            changed: vec![a.clone(), b.clone()],
            ..RestoreReport::default()
        }
    );
    assert_eq!(title(&a), strings(&["B new"]), "01 now holds B's file, untouched");
    assert_eq!(title(&b), strings(&["A new"]), "03 now holds A's file, untouched");
}

#[test]
fn a_file_written_again_after_the_apply_is_left_alone_and_the_rest_restored() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "a.flac", "A");
    let b = flac(files.path(), "b.flac", "B");
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    apply(&store, &[a.clone(), b.clone()]);
    write(&b, &titled("B later"), &MUSIC);

    let report = store.restore(&id(), &MUSIC).unwrap();

    assert_eq!(report.restored, vec![a.clone()]);
    assert_eq!(report.changed, vec![b.clone()]);
    assert_eq!(title(&a), strings(&["A"]));
    assert_eq!(title(&b), strings(&["B later"]));
}

#[test]
fn seal_keeps_only_the_written_files() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "a.flac", "A");
    let b = flac(files.path(), "b.flac", "B");
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id(), &[a.clone(), b.clone()]).unwrap();
    write(&a, &titled("A new"), &MUSIC);
    store.seal(&id(), &[(a.clone(), Fingerprint::of(&a).unwrap())]).unwrap();
    write(&b, &titled("B elsewhere"), &MUSIC);

    let report = store.restore(&id(), &MUSIC).unwrap();

    assert_eq!(report.restored, vec![a.clone()]);
    assert_eq!(title(&b), strings(&["B elsewhere"]), "b was never written by the apply");
}

#[test]
fn the_seal_survives_reopening_the_store() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "a.flac", "A");
    let b = flac(files.path(), "b.flac", "B");
    apply(
        &UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap(),
        &[a.clone(), b.clone()],
    );
    swap(&a, &b);

    let reopened = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    let report = reopened.restore(&id(), &MUSIC).unwrap();

    assert_eq!(report.changed, vec![a, b]);
    assert!(report.restored.is_empty());
}

#[test]
fn repath_of_a_sealed_record_follows_the_apply_s_own_rename() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = flac(files.path(), "a.flac", "A");
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    apply(&store, std::slice::from_ref(&a));
    let renamed = files.path().join("renamed.flac");
    fs::rename(&a, &renamed).unwrap();

    store.repath(&id(), &[(a, renamed.clone())]).unwrap();
    let report = store.restore(&id(), &MUSIC).unwrap();

    assert_eq!(report.restored, vec![renamed.clone()]);
    assert_eq!(title(&renamed), strings(&["A"]));
}
