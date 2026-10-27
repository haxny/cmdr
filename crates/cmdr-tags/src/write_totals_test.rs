//! Track and disc totals: the `TrackTotal` / `DiscTotal` fields, with `WriteOptions` as the
//! fallback when a field is absent; written only beside their number.

use std::path::Path;

use lofty::id3::v2::FrameId;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;
use lofty::tag::Accessor;

use crate::model::{FileTags, TagError, TagField};
use crate::test_files::*;
use crate::write::{WriteOptions, write_tags};
use crate::write_test::{MUSIC, raw_flac, raw_id3, raw_ilst, read, strings, write};

fn tags_of(pairs: &[(TagField, &str)]) -> FileTags {
    FileTags {
        fields: pairs.iter().map(|(f, v)| (*f, strings(&[v]))).collect(),
        ..FileTags::default()
    }
}

fn numbered() -> FileTags {
    tags_of(&[(TagField::Track, "3"), (TagField::Disc, "1")])
}

fn numbered_with_totals() -> FileTags {
    tags_of(&[
        (TagField::Track, "3"),
        (TagField::TrackTotal, "12"),
        (TagField::Disc, "1"),
        (TagField::DiscTotal, "2"),
    ])
}

const TOTALS: WriteOptions = WriteOptions {
    track_total: Some(12),
    disc_total: Some(2),
    ..MUSIC
};

/// The options' totals land in the fields, and a write without either has none.
fn with_and_without_totals(path: &Path) {
    write(path, &numbered(), &TOTALS);
    assert_eq!(read(path), numbered_with_totals(), "{path:?}");

    write(path, &numbered(), &MUSIC);
    assert_eq!(read(path), numbered(), "{path:?}");
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
fn total_field_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let padded = tags_of(&[(TagField::Track, "03"), (TagField::TrackTotal, "12")]);

    let mp3 = write_untagged_mp3(dir.path(), "a.mp3");
    write(&mp3, &padded, &MUSIC);
    assert_eq!(read(&mp3), padded);
    assert_eq!(raw_id3(&mp3).get_text(&FrameId::Valid("TRCK".into())), Some("03/12"));

    let flac = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    write(&flac, &padded, &MUSIC);
    assert_eq!(read(&flac), padded);

    // `trkn` holds integers.
    let m4a = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    write(&m4a, &padded, &MUSIC);
    assert_eq!(
        read(&m4a),
        tags_of(&[(TagField::Track, "3"), (TagField::TrackTotal, "12")])
    );
}

#[test]
fn the_field_wins_over_the_options() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    write(
        &path,
        &numbered_with_totals(),
        &WriteOptions {
            track_total: Some(99),
            disc_total: Some(98),
            ..MUSIC
        },
    );
    assert_eq!(read(&path), numbered_with_totals());
}

#[test]
fn unrelated_edit_keeps_totals() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = lofty::id3::v2::Id3v2Tag::new();
    text_frame(&mut tag, "TRCK", "3/12");
    text_frame(&mut tag, "TPOS", "1/2");
    let mp3 = write_mp3(dir.path(), "a.mp3", &tag);
    let mut ilst = Ilst::new();
    ilst.set_track(3);
    ilst.set_track_total(12);
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);
    let mut comments = VorbisComments::new();
    comments.push("TRACKNUMBER".to_owned(), "3".to_owned());
    comments.push("TOTALTRACKS".to_owned(), "12".to_owned());
    let flac = write_flac(dir.path(), "a.flac", comments, 0);

    for path in [&mp3, &m4a, &flac] {
        let mut tags = read(path);
        tags.fields.insert(TagField::Album, strings(&["Kniha"]));
        write(path, &tags, &MUSIC);
        let back = read(path);
        assert_eq!(
            back.fields.get(&TagField::TrackTotal),
            Some(&strings(&["12"])),
            "{path:?}"
        );
        assert_eq!(back, tags, "{path:?}");
    }
}

#[test]
fn no_track_drops_total() {
    let dir = tempfile::tempdir().unwrap();
    let total_only = tags_of(&[(TagField::TrackTotal, "12"), (TagField::DiscTotal, "2")]);
    for path in [
        write_flac(dir.path(), "a.flac", VorbisComments::new(), 0),
        write_untagged_mp3(dir.path(), "a.mp3"),
        write_m4a(dir.path(), "a.m4a", &Ilst::new()),
    ] {
        write(&path, &total_only, &MUSIC);
        assert_eq!(read(&path), FileTags::default(), "{path:?}");
        write(&path, &FileTags::default(), &TOTALS);
        assert_eq!(read(&path), FileTags::default(), "{path:?}");
    }
}

#[test]
fn a_total_that_isnt_a_number_fails_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let tags = tags_of(&[(TagField::Track, "3"), (TagField::TrackTotal, "dvanact")]);
    assert!(matches!(write_tags(&path, &tags, &MUSIC), Err(TagError::Write(_))));
    assert_eq!(read(&path), FileTags::default(), "nothing was written");
}

#[test]
fn totals_are_read_from_every_spelling() {
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

    assert_eq!(read(&flac), numbered_with_totals());
    assert_eq!(
        read(&m4a),
        tags_of(&[(TagField::Track, "3"), (TagField::TrackTotal, "12")])
    );
}

#[test]
fn a_total_without_its_number_is_not_checked_or_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    write(&path, &tags_of(&[(TagField::TrackTotal, "dvanact")]), &MUSIC);
    assert_eq!(read(&path), FileTags::default());
}
