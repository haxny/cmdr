//! An edit keeps every entry's name bytes as the archive stored them. Zips from
//! Japanese, Chinese, or Russian Windows tools store Shift-JIS, GBK, or CP866
//! names with the UTF-8 flag clear; a rewrite that went through a decoded name
//! re-encoded each one as CP437 mojibake flagged UTF-8, corrupting every
//! non-ASCII name in the archive (#514). Deletes and renames match the name the
//! READER shows, so a pick by its displayed name finds its entry.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::test_fixtures::{build_zip, patch_equal_len, stored};
use crate::{ArchiveByteSource, ArchiveFormat, ArchiveIndex, BytesSource};

/// `表示写真.txt` in Shift-JIS: `表` is `0x95 0x5C`, so a decoder that reads it
/// as CP437 (or treats `0x5C` as `\`) splits it into a fake folder. Not valid
/// UTF-8, so the `zip` crate falls back to CP437 for it.
const SHIFT_JIS_NAME: &[u8] = b"\x95\x5c\x8e\xa6\x8e\xca\x90\x5e.txt";
/// What the reader shows for [`SHIFT_JIS_NAME`].
const SHIFT_JIS_SHOWN: &str = "表示写真.txt";
/// An ASCII stand-in of the same length, patched to [`SHIFT_JIS_NAME`].
const SHIFT_JIS_PLACEHOLDER: &[u8] = b"abcdefgh.txt";

/// UTF-8 bytes stored WITHOUT the UTF-8 flag, as older Info-ZIP on Linux writes.
const UNFLAGGED_UTF8_NAME: &[u8] = "café.txt".as_bytes();
const UNFLAGGED_UTF8_PLACEHOLDER: &[u8] = b"cafe_.txt";

/// General-purpose flag bit 11: the name and comment are UTF-8.
const GP_FLAG_UTF8: u16 = 1 << 11;

/// One entry's name bytes and general-purpose flags as the central directory
/// stores them.
#[derive(Debug, PartialEq, Eq)]
struct StoredName {
    raw: Vec<u8>,
    flags: u16,
}

fn stored_names(path: &Path) -> Vec<StoredName> {
    let mut archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    (0..archive.len())
        .map(|i| {
            let file = archive.by_index_raw(i).expect("a raw entry");
            StoredName {
                raw: file.name_raw().to_vec(),
                flags: file.flags().as_u16(),
            }
        })
        .collect()
}

fn stored_name(path: &Path, raw: &[u8]) -> Option<StoredName> {
    stored_names(path).into_iter().find(|name| name.raw == raw)
}

/// The top-level names Cmdr's reader shows for the zip at `path`.
fn shown_names(path: &Path) -> Vec<String> {
    let source: Arc<dyn ArchiveByteSource> = Arc::new(BytesSource::new(std::fs::read(path).expect("read the zip")));
    let index = ArchiveIndex::parse(source, ArchiveFormat::Zip, None).expect("the zip browses");
    let mut names: Vec<String> = index
        .list("")
        .expect("the root lists")
        .into_iter()
        .map(|node| node.name.clone())
        .collect();
    names.sort();
    names
}

/// A zip with a Shift-JIS entry (UTF-8 flag clear) beside `others`.
fn shift_jis_zip(dir: &Path, others: &[&str]) -> PathBuf {
    let mut entries = vec![stored(
        String::from_utf8(SHIFT_JIS_PLACEHOLDER.to_vec()).expect("ASCII"),
        b"shift-jis body".to_vec(),
    )];
    entries.extend(others.iter().map(|name| stored(*name, b"other body".to_vec())));
    let mut bytes = build_zip(&entries);
    patch_equal_len(&mut bytes, SHIFT_JIS_PLACEHOLDER, SHIFT_JIS_NAME);
    let path = dir.join("sjis.zip");
    std::fs::write(&path, bytes).expect("write the fixture");
    path
}

fn delete(path: &Path, inner: &str) {
    let changeset = Changeset {
        deletes: vec![inner.to_string()],
        ..Default::default()
    };
    apply(path, &changeset, &NoHooks).expect("the delete applies");
}

#[test]
fn deleting_a_sibling_keeps_a_shift_jis_names_bytes_and_flags() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = shift_jis_zip(tmp.path(), &["ascii.txt"]);
    // The precondition the rest rests on: the reader decodes the name right.
    assert_eq!(shown_names(&path), vec!["ascii.txt", SHIFT_JIS_SHOWN]);
    let before = stored_name(&path, SHIFT_JIS_NAME).expect("the Shift-JIS entry is there");
    assert_eq!(
        before.flags & GP_FLAG_UTF8,
        0,
        "the fixture leaves the UTF-8 flag clear"
    );

    delete(&path, "ascii.txt");

    assert_eq!(
        stored_names(&path),
        vec![before],
        "the name bytes and flags survive as stored"
    );
    assert_eq!(shown_names(&path), vec![SHIFT_JIS_SHOWN]);
    // The local header carries the same bytes as the central directory.
    let bytes = std::fs::read(&path).expect("read the result");
    let copies = bytes
        .windows(SHIFT_JIS_NAME.len())
        .filter(|w| *w == SHIFT_JIS_NAME)
        .count();
    assert_eq!(
        copies, 2,
        "the local header and the central directory both keep the raw name"
    );
}

#[test]
fn a_shift_jis_entry_deletes_by_the_name_the_reader_shows() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = shift_jis_zip(tmp.path(), &["ascii.txt"]);

    delete(&path, SHIFT_JIS_SHOWN);

    assert_eq!(shown_names(&path), vec!["ascii.txt"]);
}

#[test]
fn a_shift_jis_entry_renames_by_the_name_the_reader_shows() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = shift_jis_zip(tmp.path(), &["ascii.txt"]);

    let changeset = Changeset {
        renames: vec![(SHIFT_JIS_SHOWN.to_string(), "写真.txt".to_string())],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("the rename applies");

    assert_eq!(shown_names(&path), vec!["ascii.txt", "写真.txt"]);
    // The new name is the user's text, so it's written as flagged UTF-8.
    let renamed = stored_name(&path, "写真.txt".as_bytes()).expect("the renamed entry is UTF-8");
    assert_ne!(renamed.flags & GP_FLAG_UTF8, 0);
    assert_eq!(read_entry(&path, "写真.txt"), b"shift-jis body");
}

#[test]
fn renaming_a_folder_moves_its_shift_jis_children_under_the_shown_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // `dirxxxxx/` is patched to Shift-JIS `写真/` together with its child's prefix.
    let mut bytes = build_zip(&[
        stored("dirx/abcdefgh.txt", b"child".to_vec()),
        stored("keep.txt", b"k".to_vec()),
    ]);
    patch_equal_len(
        &mut bytes,
        b"dirx/abcdefgh.txt",
        b"\x8e\xca\x90\x5e/\x95\x5c\x8e\xa6\x8e\xca\x90\x5e.txt",
    );
    let path = tmp.path().join("nested.zip");
    std::fs::write(&path, bytes).expect("write the fixture");
    assert_eq!(shown_names(&path), vec!["keep.txt", "写真"]);

    let changeset = Changeset {
        renames: vec![("写真".to_string(), "photos".to_string())],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("the rename applies");

    assert_eq!(read_entry(&path, "photos/表示写真.txt"), b"child");
    let kept = stored_name(&path, b"keep.txt").expect("the untouched entry stays");
    assert_eq!(kept.flags & GP_FLAG_UTF8, 0);
}

#[test]
fn an_unflagged_utf8_name_keeps_its_flag_clear_when_retained() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut bytes = build_zip(&[
        stored(
            String::from_utf8(UNFLAGGED_UTF8_PLACEHOLDER.to_vec()).expect("ASCII"),
            b"cafe".to_vec(),
        ),
        stored("ascii.txt", b"other".to_vec()),
    ]);
    patch_equal_len(&mut bytes, UNFLAGGED_UTF8_PLACEHOLDER, UNFLAGGED_UTF8_NAME);
    let path = tmp.path().join("unflagged.zip");
    std::fs::write(&path, bytes).expect("write the fixture");
    let before = stored_name(&path, UNFLAGGED_UTF8_NAME).expect("the entry is there");
    assert_eq!(
        before.flags & GP_FLAG_UTF8,
        0,
        "the fixture leaves the UTF-8 flag clear"
    );

    delete(&path, "ascii.txt");

    assert_eq!(
        stored_names(&path),
        vec![before],
        "a retained entry's flags stay as stored"
    );
}

/// The two crates key an entry by the same local header offset even when bytes
/// precede the archive (a self-extractor stub), so the shown names still pair up.
#[test]
fn shown_names_pair_up_with_the_writers_entries_behind_a_prefix() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = shift_jis_zip(tmp.path(), &["ascii.txt"]);
    let mut bytes = vec![0u8; 100];
    bytes.extend(std::fs::read(&path).expect("read the fixture"));
    std::fs::write(&path, &bytes).expect("write the prefixed fixture");

    let shown = crate::mutation::plan::shown_names(&path);
    let mut archive = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");
    let paired: Vec<Option<&str>> = (0..archive.len())
        .map(|i| {
            let file = archive.by_index_raw(i).expect("a raw entry");
            shown.get(&file.header_start()).map(String::as_str)
        })
        .collect();
    assert_eq!(paired, vec![Some(SHIFT_JIS_SHOWN), Some("ascii.txt")]);
}

fn read_entry(path: &Path, name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    let mut entry = archive.by_name(name).expect("the entry is there");
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).expect("read the entry");
    buf
}
