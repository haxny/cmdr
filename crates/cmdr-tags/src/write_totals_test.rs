//! Track and disc totals: written only when the caller passes them, and read back raw.

use std::path::Path;

use lofty::id3::v2::FrameId;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;
use lofty::tag::Accessor;

use crate::model::{FileTags, TagField};
use crate::read::{RawTags, read_raw};
use crate::test_files::*;
use crate::write::WriteOptions;
use crate::write_test::{MUSIC, raw_flac, raw_id3, raw_ilst, read, strings, write};

fn numbered() -> FileTags {
    FileTags {
        fields: [(TagField::Track, strings(&["3"])), (TagField::Disc, strings(&["1"]))].into(),
        ..FileTags::default()
    }
}

const TOTALS: WriteOptions = WriteOptions {
    track_total: Some(12),
    disc_total: Some(2),
    ..MUSIC
};

fn totals(track: Option<u32>, disc: Option<u32>) -> RawTags {
    RawTags {
        track_total: track,
        disc_total: disc,
        conductor: None,
    }
}

fn with_and_without_totals(path: &Path) {
    write(path, &numbered(), &TOTALS);
    assert_eq!(read(path), numbered(), "{path:?}: totals are no fields or extras");
    assert_eq!(read_raw(path).unwrap(), totals(Some(12), Some(2)), "{path:?}");

    write(path, &numbered(), &MUSIC);
    assert_eq!(read_raw(path).unwrap(), totals(None, None), "{path:?}");
}

#[test]
fn mp3_disc_total_goes_in_tpos() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    write(&path, &numbered(), &TOTALS);
    let raw = raw_id3(&path);
    assert_eq!(raw.get_text(&FrameId::Valid("TPOS".into())), Some("1/2"));
    assert_eq!(raw.get_text(&FrameId::Valid("TRCK".into())), Some("3/12"));
    with_and_without_totals(&path);
}

#[test]
fn m4a_disc_total_goes_in_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    write(&path, &numbered(), &TOTALS);
    assert_eq!(raw_ilst(&path).disk_total(), Some(2));
    with_and_without_totals(&path);
}

#[test]
fn vorbis_disc_total_is_a_disctotal_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    write(&path, &numbered(), &TOTALS);
    let raw = raw_flac(&path);
    let comments = raw.vorbis_comments().unwrap();
    assert_eq!(comments.get_all("DISCNUMBER").collect::<Vec<_>>(), ["1"]);
    assert_eq!(comments.get_all("DISCTOTAL").collect::<Vec<_>>(), ["2"]);
    with_and_without_totals(&path);
    let ogg = write_ogg_vorbis(dir.path(), "a.ogg", VorbisComments::new(), 0);
    with_and_without_totals(&ogg);
}

#[test]
fn no_total_without_its_number() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    write(&path, &FileTags::default(), &TOTALS);
    assert_eq!(read_raw(&path).unwrap(), totals(None, None));
}

#[test]
fn raw_totals_are_read_from_every_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.push("TRACKNUMBER".to_owned(), "3/12".to_owned());
    comments.push("DISCNUMBER".to_owned(), "1".to_owned());
    comments.push("TOTALDISCS".to_owned(), "2".to_owned());
    let flac = write_flac(dir.path(), "a.flac", comments, 0);
    let mut ilst = Ilst::new();
    ilst.set_track(3);
    ilst.set_track_total(12);
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);

    assert_eq!(read_raw(&flac).unwrap(), totals(Some(12), Some(2)));
    assert_eq!(read_raw(&m4a).unwrap(), totals(Some(12), None));
}
