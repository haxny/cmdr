//! The undo store: record a snapshot, change the files, restore, and read the old tags back.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;
use lofty::tag::Accessor;

use crate::model::{FileTags, TagError, TagField};
use crate::read::{RawTags, read_raw};
use crate::test_files::*;
use crate::undo::{ApplyId, RestoreReport, UndoLimits, UndoStore};
use crate::write_test::{BOOK, MUSIC, jpeg, listing, read, strings, write};

const ROOMY: UndoLimits = UndoLimits {
    max_applies: 50,
    max_bytes: u64::MAX,
};

fn id(name: &str) -> ApplyId {
    ApplyId(name.to_owned())
}

/// Three files, each with a cover, a title, and an extra.
fn tagged_files(dir: &Path) -> Vec<PathBuf> {
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", "Puvodni");
    txxx(&mut tag, "ITUNESMEDIATYPE", "Audiobook");
    id3_cover(&mut tag, 40);
    let mp3 = write_mp3(dir, "a.mp3", &tag);

    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9nam", "Puvodni");
    freeform(&mut ilst, "com.apple.iTunes", "ITUNESMEDIATYPE", &["Audiobook"]);
    ilst.insert_picture(cmdr_media_meta::test_fixtures::cover(50));
    let m4b = write_m4a(dir, "b.m4b", &ilst);

    let mut comments = VorbisComments::new();
    comments.push("TITLE".to_owned(), "Puvodni".to_owned());
    comments.push("ITUNESMEDIATYPE".to_owned(), "Audiobook".to_owned());
    let flac = write_flac(dir, "c.flac", comments, 60);

    vec![mp3, m4b, flac]
}

fn snapshot(paths: &[PathBuf]) -> Vec<(PathBuf, FileTags)> {
    paths.iter().map(|p| (p.clone(), read(p))).collect()
}

/// What an apply does: a new title, the cover and the extras gone.
fn change(path: &Path) {
    let mut tags = read(path);
    tags.fields.insert(TagField::Title, strings(&["Zmeneno"]));
    tags.cover = None;
    tags.extras.clear();
    write(path, &tags, &MUSIC);
}

#[test]
fn restore_puts_the_snapshot_back_cover_and_removed_extra_included() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let before = snapshot(&paths);
    assert!(before.iter().all(|(_, t)| t.cover.is_some() && !t.extras.is_empty()));
    let store = UndoStore::open(root.path().join("undo"), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    for path in &paths {
        change(path);
        assert_ne!(read(path), before.iter().find(|(p, _)| p == path).unwrap().1);
    }

    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(
        report,
        RestoreReport {
            restored: paths.clone(),
            ..RestoreReport::default()
        }
    );
    for (path, tags) in &before {
        assert_eq!(read(path), *tags, "{path:?}");
        assert_eq!(
            read_raw(path).unwrap(),
            RawTags::default(),
            "{path:?}: no totals appear"
        );
    }
}

#[test]
fn record_keeps_covers_beside_the_manifest_not_in_it() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = write_flac(files.path(), "a.flac", VorbisComments::new(), 10_000);
    let cover = read(&path).cover.unwrap().data;
    assert_eq!(cover.len(), 10_000);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();

    store.record(&id("op-1"), &[path]).unwrap();

    let apply = root.path().join("op-1");
    assert_eq!(listing(&apply), strings(&["0.cover", "manifest.json"]));
    assert_eq!(fs::read(apply.join("0.cover")).unwrap(), cover);
    let manifest = fs::read(apply.join("manifest.json")).unwrap();
    assert!(manifest.len() < 1000, "{} bytes", manifest.len());
    let json: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(
        json["entries"][0]["cover"],
        serde_json::json!({"file": "0.cover", "mime": "image/jpeg"})
    );
}

#[test]
fn entries_with_the_same_cover_share_one_file() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths: Vec<PathBuf> = (1..=3)
        .map(|n| write_flac(files.path(), &format!("{n}.flac"), VorbisComments::new(), 5_000))
        .collect();
    let before = snapshot(&paths);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();

    store.record(&id("op-1"), &paths).unwrap();
    for path in &paths {
        change(path);
    }
    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(
        listing(&root.path().join("op-1")),
        strings(&["0.cover", "manifest.json"])
    );
    assert_eq!(report.restored, paths);
    for (path, tags) in &before {
        assert_eq!(read(path), *tags);
    }
}

#[test]
fn a_file_without_a_cover_gets_its_cover_removed_again() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = write_flac(files.path(), "a.flac", VorbisComments::new(), 0);
    let before = snapshot(std::slice::from_ref(&path));
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), std::slice::from_ref(&path)).unwrap();
    write(
        &path,
        &FileTags {
            cover: Some(jpeg(30, 0x11)),
            ..read(&path)
        },
        &MUSIC,
    );

    store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(read(&path), before[0].1);
    assert_eq!(listing(&root.path().join("op-1")), strings(&["manifest.json"]));
}

#[test]
fn restore_follows_renames_recorded_with_repath() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let before = snapshot(&paths);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    let renamed = files.path().join("renamed.mp3");
    change(&paths[0]);
    fs::rename(&paths[0], &renamed).unwrap();

    let elsewhere = files.path().join("never-recorded.mp3");
    store
        .repath(
            &id("op-1"),
            &[
                (paths[0].clone(), renamed.clone()),
                (elsewhere.clone(), paths[1].clone()),
            ],
        )
        .unwrap();
    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(
        report.restored,
        vec![renamed.clone(), paths[1].clone(), paths[2].clone()]
    );
    assert_eq!(read(&renamed), before[0].1);
}

#[test]
fn repath_swaps_two_names() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = write_flac(files.path(), "a.flac", VorbisComments::new(), 0);
    write(
        &a,
        &FileTags {
            fields: [(TagField::Title, strings(&["A"]))].into(),
            ..FileTags::default()
        },
        &MUSIC,
    );
    let b = write_flac(files.path(), "b.flac", VorbisComments::new(), 0);
    write(
        &b,
        &FileTags {
            fields: [(TagField::Title, strings(&["B"]))].into(),
            ..FileTags::default()
        },
        &MUSIC,
    );
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &[a.clone(), b.clone()]).unwrap();
    // The apply swapped the two files' names.
    let tmp = files.path().join("swap.flac");
    fs::rename(&a, &tmp).unwrap();
    fs::rename(&b, &a).unwrap();
    fs::rename(&tmp, &b).unwrap();

    store
        .repath(&id("op-1"), &[(a.clone(), b.clone()), (b.clone(), a.clone())])
        .unwrap();
    store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(read(&b).fields[&TagField::Title], strings(&["A"]));
    assert_eq!(read(&a).fields[&TagField::Title], strings(&["B"]));
}

#[test]
fn a_deleted_file_is_missing_and_a_failing_one_does_not_stop_the_rest() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let before = snapshot(&paths);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    fs::remove_file(&paths[0]).unwrap();
    fs::write(&paths[1], b"no longer an M4B").unwrap();
    change(&paths[2]);

    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(report.missing, vec![paths[0].clone()]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, paths[1]);
    assert!(
        matches!(report.failed[0].1, TagError::Parse(_)),
        "{:?}",
        report.failed[0].1
    );
    assert_eq!(report.restored, vec![paths[2].clone()]);
    assert_eq!(read(&paths[2]), before[2].1);
}

#[test]
fn restore_leaves_the_record_until_forget() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();

    store.restore(&id("op-1"), &MUSIC).unwrap();
    store.restore(&id("op-1"), &MUSIC).unwrap();
    store.forget(&id("op-1")).unwrap();

    assert!(!root.path().join("op-1").exists());
    let err = store.restore(&id("op-1"), &MUSIC).unwrap_err();
    assert!(matches!(err, TagError::Io(ErrorKind::NotFound, _)), "{err:?}");
    store.forget(&id("op-1")).unwrap();
}

#[test]
fn recording_an_existing_id_replaces_it() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    change(&paths[2]);
    let second = snapshot(&paths[2..]);

    store.record(&id("op-1"), &paths[2..]).unwrap();
    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(report.restored, vec![paths[2].clone()]);
    assert_eq!(listing(&root.path().join("op-1")), strings(&["manifest.json"]));
    assert_eq!(read(&paths[2]), second[0].1);
}

#[test]
fn pruning_keeps_the_newest_applies() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let limits = UndoLimits {
        max_applies: 2,
        max_bytes: u64::MAX,
    };
    let store = UndoStore::open(root.path().to_path_buf(), limits).unwrap();

    for name in ["op-a", "op-b", "op-c"] {
        store.record(&id(name), &paths[2..]).unwrap();
    }

    assert_eq!(listing(root.path()), strings(&["op-b", "op-c"]));
    // Order is by recording, not by name.
    store.record(&id("op-0"), &paths[2..]).unwrap();
    assert_eq!(listing(root.path()), strings(&["op-0", "op-c"]));
}

#[test]
fn pruning_by_size_never_drops_the_apply_just_recorded() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let limits = UndoLimits {
        max_applies: 50,
        max_bytes: 10,
    };
    let store = UndoStore::open(root.path().to_path_buf(), limits).unwrap();

    store.record(&id("op-a"), &paths).unwrap();
    store.record(&id("op-b"), &paths).unwrap();

    assert_eq!(listing(root.path()), strings(&["op-b"]));
    store.restore(&id("op-b"), &MUSIC).unwrap();
}

#[test]
fn a_half_written_manifest_is_ignored_and_cleaned_on_open() {
    let root = tempfile::tempdir().unwrap();
    let apply = root.path().join("op-1");
    fs::create_dir(&apply).unwrap();
    fs::write(apply.join("manifest.json.tmp"), b"{\"version\":1,").unwrap();
    fs::write(apply.join("0.cover"), b"half").unwrap();

    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();

    assert!(!apply.join("manifest.json.tmp").exists());
    let err = store.restore(&id("op-1"), &MUSIC).unwrap_err();
    assert!(matches!(err, TagError::Io(ErrorKind::NotFound, _)), "{err:?}");
    let err = store.repath(&id("op-1"), &[]).unwrap_err();
    assert!(matches!(err, TagError::Io(ErrorKind::NotFound, _)), "{err:?}");
}

#[test]
fn a_damaged_manifest_is_a_parse_error() {
    let root = tempfile::tempdir().unwrap();
    let apply = root.path().join("op-1");
    fs::create_dir(&apply).unwrap();
    fs::write(apply.join("manifest.json"), b"{\"version\":1,").unwrap();
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();

    let err = store.restore(&id("op-1"), &MUSIC).unwrap_err();

    assert!(matches!(err, TagError::Parse(_)), "{err:?}");
}

#[test]
fn open_creates_the_root() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("a").join("undo");

    UndoStore::open(nested.clone(), UndoLimits::default()).unwrap();

    assert!(nested.is_dir());
}

#[test]
fn default_limits() {
    assert_eq!(
        UndoLimits::default(),
        UndoLimits {
            max_applies: 50,
            max_bytes: 512 * 1024 * 1024,
        }
    );
}

#[test]
fn an_id_must_be_one_safe_path_component() {
    let root = tempfile::tempdir().unwrap();
    let store = UndoStore::open(root.path().join("undo"), ROOMY).unwrap();
    for bad in ["", ".", "..", "../x", "a/b", "/abs", ".hidden", "a..b", "a\\b", "a\0b"] {
        let bad = id(bad);
        for result in [
            store.record(&bad, &[]),
            store.repath(&bad, &[]),
            store.restore(&bad, &MUSIC).map(|_| ()),
            store.forget(&bad),
        ] {
            assert!(
                matches!(result, Err(TagError::Io(ErrorKind::InvalidInput, _))),
                "{bad:?}: {result:?}"
            );
        }
    }
    assert_eq!(listing(root.path()), strings(&["undo"]));
    assert!(listing(&root.path().join("undo")).is_empty());
}

#[test]
fn restore_brings_back_track_and_disc_totals() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TRCK", "3/12");
    text_frame(&mut tag, "TPOS", "1/2");
    let mp3 = write_mp3(files.path(), "a.mp3", &tag);
    let mut ilst = Ilst::new();
    ilst.set_track(3);
    ilst.set_track_total(12);
    ilst.set_disk(1);
    ilst.set_disk_total(2);
    let m4b = write_m4a(files.path(), "b.m4b", &ilst);
    let mut comments = VorbisComments::new();
    for (key, value) in [
        ("TRACKNUMBER", "3"),
        ("TRACKTOTAL", "12"),
        ("DISCNUMBER", "1"),
        ("DISCTOTAL", "2"),
    ] {
        comments.push(key.to_owned(), value.to_owned());
    }
    let flac = write_flac(files.path(), "c.flac", comments, 0);
    let paths = vec![mp3, m4b, flac];
    let totals = RawTags {
        track_total: Some(12),
        disc_total: Some(2),
        conductor: None,
    };
    let before = snapshot(&paths);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    for path in &paths {
        assert_eq!(read_raw(path).unwrap(), totals, "{path:?}");
        write(path, &read(path), &MUSIC);
        assert_eq!(
            read_raw(path).unwrap(),
            RawTags::default(),
            "{path:?}: the apply dropped the totals"
        );
    }

    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(report.restored, paths);
    for (path, tags) in &before {
        assert_eq!(read(path), *tags, "{path:?}");
        assert_eq!(read_raw(path).unwrap(), totals, "{path:?}");
    }
}

#[test]
fn restore_after_an_audiobook_apply_brings_back_composer_and_conductor() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TCOM", "Skladatel");
    text_frame(&mut tag, "TPE3", "Dirigent");
    let mp3 = write_mp3(files.path(), "a.mp3", &tag);
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9wrt", "Skladatel");
    let m4b = write_m4a(files.path(), "b.m4b", &ilst);
    let mut comments = VorbisComments::new();
    comments.push("COMPOSER".to_owned(), "Skladatel".to_owned());
    let flac = write_flac(files.path(), "c.flac", comments, 0);
    let paths = vec![mp3.clone(), m4b, flac];
    let before = snapshot(&paths);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), &paths).unwrap();
    for path in &paths {
        write(path, &read(path), &BOOK);
        assert!(!read(path).fields.contains_key(&TagField::Composer), "{path:?}");
    }
    assert_eq!(read_raw(&mp3).unwrap().conductor, None);

    // The caller's kind doesn't matter: a snapshot goes back as it was.
    let report = store.restore(&id("op-1"), &BOOK).unwrap();

    assert_eq!(report.restored, paths);
    for (path, tags) in &before {
        assert_eq!(read(path), *tags, "{path:?}");
    }
    assert_eq!(read_raw(&mp3).unwrap().conductor.as_deref(), Some("Dirigent"));
}

#[test]
fn an_extra_that_cannot_come_back_fails_the_file() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(&mut ilst, "org.example", "PERFORMER", &["Cizi"]);
    let path = write_m4a(files.path(), "a.m4a", &ilst);
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    store.record(&id("op-1"), std::slice::from_ref(&path)).unwrap();
    change(&path);

    let report = store.restore(&id("op-1"), &MUSIC).unwrap();

    assert_eq!(
        report,
        RestoreReport {
            failed: vec![(path, TagError::UnwritableExtra("PERFORMER".to_owned()))],
            ..RestoreReport::default()
        }
    );
}

#[test]
fn open_removes_a_leftover_staging_record_and_only_the_store_s_temp_files() {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join(".op-1.staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("manifest.json"), b"{}").unwrap();
    let apply = root.path().join("op-2");
    fs::create_dir(&apply).unwrap();
    for name in [
        "manifest.json.aB3x9Z.tmp",
        "0.cover.Qr7tY2.tmp",
        "1.cover.tmp",
        "notes.tmp",
        "x.cover.a.b.tmp",
    ] {
        fs::write(apply.join(name), b"").unwrap();
    }
    let elsewhere = root.path().join("keep.tmp");
    fs::write(&elsewhere, b"").unwrap();

    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();

    assert!(!staging.exists());
    assert_eq!(listing(&apply), strings(&["notes.tmp", "x.cover.a.b.tmp"]));
    assert!(elsewhere.exists());
    let err = store.restore(&id("op-1"), &MUSIC).unwrap_err();
    assert!(matches!(err, TagError::Io(ErrorKind::NotFound, _)), "{err:?}");
}

#[test]
fn a_staging_record_is_neither_an_apply_nor_counted() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let limits = UndoLimits {
        max_applies: 1,
        max_bytes: u64::MAX,
    };
    let store = UndoStore::open(root.path().to_path_buf(), limits).unwrap();
    store.record(&id("op-a"), &paths[2..]).unwrap();
    // A record another call is still building.
    let staging = root.path().join(".op-z.staging");
    fs::create_dir(&staging).unwrap();
    fs::copy(root.path().join("op-a/manifest.json"), staging.join("manifest.json")).unwrap();

    store.record(&id("op-b"), &paths[2..]).unwrap();

    assert_eq!(listing(root.path()), strings(&[".op-z.staging", "op-b"]));
}

#[test]
fn record_reads_the_files_itself_and_stores_nothing_when_one_cannot_be_read() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    let store = UndoStore::open(root.path().to_path_buf(), ROOMY).unwrap();
    let gone = files.path().join("gone.mp3");

    let err = store.record(&id("op-1"), &[paths[0].clone(), gone]).unwrap_err();

    assert!(matches!(err, TagError::Io(ErrorKind::NotFound, _)), "{err:?}");
    assert!(listing(root.path()).is_empty());
}

/// An apply directory whose manifest this store can't load, last modified `age_secs` ago.
fn unloadable_apply(root: &Path, name: &str, manifest: &[u8], junk: usize, age_secs: u64) {
    let dir = root.join(name);
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("manifest.json"), manifest).unwrap();
    fs::write(dir.join("0.cover"), vec![0u8; junk]).unwrap();
    let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
    fs::File::open(&dir).unwrap().set_modified(when).unwrap();
}

#[test]
fn damaged_and_unknown_version_applies_count_and_go_first_oldest_first() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    unloadable_apply(root.path(), "op-damaged", b"{\"version\":1,", 10, 300);
    unloadable_apply(
        root.path(),
        "op-future",
        b"{\"version\":9,\"seq\":7,\"entries\":[]}",
        10,
        100,
    );
    let limits = UndoLimits {
        max_applies: 3,
        max_bytes: u64::MAX,
    };
    let store = UndoStore::open(root.path().to_path_buf(), limits).unwrap();
    store.record(&id("op-a"), &paths[2..]).unwrap();
    assert_eq!(
        listing(root.path()),
        strings(&["op-a", "op-damaged", "op-future"]),
        "three within the limit"
    );

    store.record(&id("op-b"), &paths[2..]).unwrap();

    assert_eq!(listing(root.path()), strings(&["op-a", "op-b", "op-future"]));
}

#[test]
fn a_large_damaged_apply_is_pruned_by_size_but_never_the_new_record() {
    let files = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = tagged_files(files.path());
    unloadable_apply(root.path(), "op-damaged", b"not json", 1_000_000, 60);
    let limits = UndoLimits {
        max_applies: 50,
        max_bytes: 100_000,
    };
    let store = UndoStore::open(root.path().to_path_buf(), limits).unwrap();

    store.record(&id("op-a"), &paths).unwrap();

    assert_eq!(listing(root.path()), strings(&["op-a"]));
    store.restore(&id("op-a"), &MUSIC).unwrap();
}
