//! An edit keeps what the archive is beyond its visible entries: the archive
//! comment, the bytes before the zip data (a self-extractor stub), and a
//! symlink the archive was opened through. One it can't keep, the earlier of
//! two entries sharing a name, makes it refuse the edit untouched (#542).

use std::path::{Path, PathBuf};

use super::*;
use crate::test_fixtures::{build_zip, build_zipcrypto_zip, plain_entry, stored};

fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write the fixture zip");
    path
}

fn delete(path: &Path, inner: &str) -> Result<(), MutationError> {
    let changeset = Changeset {
        deletes: vec![inner.to_string()],
        ..Default::default()
    };
    apply(path, &changeset, &NoHooks)
}

fn entry_names(path: &Path) -> Vec<String> {
    let archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    let mut names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a name").into_owned())
        .collect();
    names.sort();
    names
}

fn leftover_temps(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("list the dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect()
}

#[test]
fn an_edit_keeps_the_archive_comment() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
    for name in ["keep.txt", "drop.txt"] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start an entry");
        writer.write_all(b"bytes").expect("write an entry");
    }
    writer
        .set_raw_comment(Box::from(&b"Release 1.2, built 2026-10-20"[..]))
        .expect("set the comment");
    let path = write_zip(tmp.path(), "a.zip", &writer.finish().expect("finish").into_inner());

    delete(&path, "drop.txt").expect("the edit applies");

    let archive = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");
    assert_eq!(archive.comment(), b"Release 1.2, built 2026-10-20");
    assert_eq!(entry_names(&path), vec!["keep.txt"]);
}

/// The stub before the zip data is what makes a self-extractor run; the edit
/// writes it back first and the zip after it, with offsets that count it.
#[test]
fn an_edit_keeps_the_bytes_before_the_zip_data() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let stub = b"#!/bin/sh\necho 'a self-extractor stub'\nexit 0\n".repeat(3);
    let mut bytes = stub.clone();
    bytes.extend(build_zip(&[
        stored("keep.txt", "keep me"),
        stored("drop.txt", "drop me"),
    ]));
    let path = write_zip(tmp.path(), "sfx.zip", &bytes);

    delete(&path, "drop.txt").expect("the edit applies");

    let after = std::fs::read(&path).expect("read the result");
    assert!(after.starts_with(&stub), "the stub is still at the start");
    assert_eq!(entry_names(&path), vec!["keep.txt"]);
    let source = std::sync::Arc::new(crate::BytesSource::new(after));
    let index = crate::ArchiveIndex::parse(source, crate::ArchiveFormat::Zip, None).expect("the reader parses it");
    assert!(index.get("keep.txt").is_some(), "the reader still finds the kept entry");
    let unzip_ok = std::process::Command::new("unzip")
        .arg("-t")
        .arg(&path)
        .output()
        .map_or(true, |output| output.status.success());
    assert!(unzip_ok, "unzip -t accepts the result");
}

/// The `zip` crate folds a duplicate name into one entry, so a rewrite through
/// it can't keep the earlier copy. Refused before any temp exists.
#[test]
fn an_archive_with_a_duplicate_name_is_refused_untouched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bytes = build_zipcrypto_zip(
        &[
            plain_entry("twin.txt", "earlier twin"),
            plain_entry("other.txt", "other"),
            plain_entry("twin.txt", "later twin"),
        ],
        "unused",
    );
    let path = write_zip(tmp.path(), "twins.zip", &bytes);

    let result = delete(&path, "other.txt");

    assert!(
        matches!(&result, Err(MutationError::DuplicateEntryName { name }) if name == "twin.txt"),
        "got {result:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("read the original"),
        bytes,
        "the original is untouched"
    );
    assert!(leftover_temps(tmp.path()).is_empty(), "no temp is left behind");
}

/// The rename replaces the file the link points at, so the edit lands where the
/// user's archive lives and the link stays a link.
#[cfg(unix)]
#[test]
fn an_edit_through_a_symlink_changes_the_target_and_keeps_the_link() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).expect("make the target dir");
    let target = write_zip(
        &real_dir,
        "report.zip",
        &build_zip(&[stored("keep.txt", "keep me"), stored("drop.txt", "drop me")]),
    );
    let link = tmp.path().join("report.zip");
    std::os::unix::fs::symlink(&target, &link).expect("make the link");

    delete(&link, "drop.txt").expect("the edit applies");

    let link_meta = std::fs::symlink_metadata(&link).expect("stat the link");
    assert!(link_meta.file_type().is_symlink(), "the link is still a link");
    assert_eq!(std::fs::read_link(&link).expect("read the link"), target);
    assert_eq!(entry_names(&target), vec!["keep.txt"], "the target holds the edit");
    assert!(leftover_temps(tmp.path()).is_empty() && leftover_temps(&real_dir).is_empty());
}
