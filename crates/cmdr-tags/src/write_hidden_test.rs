//! Values the reader hides (Ruling 25): a second value of a single-value field, a legacy
//! `TXXX:Comment` / `TXXX:DiscNumber` beside `COMM` / `TPOS`, a `COMM` in another language, a
//! repeated Vorbis key, a `©gen` beside the freeform `GENRE`. A write removes them, so one that
//! differs from what the reader shows refuses as `WouldDrop` unless the caller allows the loss.
//! Also the Vorbis `ALBUM ARTIST` alias and `"; "`-joined Vorbis values (Ruling 27).

use std::fs;
use std::path::Path;

use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;

use crate::model::{FileTags, TagError, TagField};
use crate::test_files::*;
use crate::write::{WriteOptions, write_tags};
use crate::write_test::{
    MUSIC, fourcc, freeform_data, raw_flac, raw_id3, raw_ilst, read, strings, txxx_descriptions, utf8, write,
};

const LOSSY: WriteOptions = WriteOptions {
    allow_frame_loss: true,
    ..MUSIC
};

fn retitled(path: &Path) -> FileTags {
    let mut tags = read(path);
    tags.fields.insert(TagField::Title, strings(&["Novy nazev"]));
    tags
}

/// Writes a new title and expects a refusal naming `ids`, with the file untouched.
fn refused(path: &Path, ids: &[&str]) {
    let bytes = fs::read(path).unwrap();
    let err = write_tags(path, &retitled(path), &MUSIC).unwrap_err();
    assert_eq!(err, TagError::WouldDrop(strings(ids)));
    assert_eq!(fs::read(path).unwrap(), bytes, "the file is untouched");
}

fn vorbis(path: &Path) -> Vec<String> {
    let comments = raw_flac(path).vorbis_comments().cloned().unwrap_or_default();
    comments.items().map(|(k, v)| format!("{k}={v}")).collect()
}

fn flac_with(dir: &Path, items: &[(&str, &str)]) -> std::path::PathBuf {
    let mut comments = VorbisComments::new();
    for (key, value) in items {
        comments.push((*key).to_owned(), (*value).to_owned());
    }
    write_flac(dir, "a.flac", comments, 0)
}

#[test]
fn a_differing_legacy_txxx_comment_beside_comm_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm(&mut tag, "Ripped by X");
    txxx(&mut tag, "Comment", "Dlouhy popis knihy");
    let path = write_mp3(dir.path(), "a.mp3", &tag);
    assert_eq!(read(&path).fields[&TagField::Comment], strings(&["Ripped by X"]));

    refused(&path, &["TXXX:Comment"]);

    write(&path, &retitled(&path), &LOSSY);
    let tags = read(&path);
    assert_eq!(tags.fields[&TagField::Title], strings(&["Novy nazev"]));
    assert_eq!(tags.fields[&TagField::Comment], strings(&["Ripped by X"]));
    assert!(txxx_descriptions(&raw_id3(&path)).is_empty());
}

#[test]
fn a_differing_legacy_txxx_discnumber_beside_tpos_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TPOS", "1");
    txxx(&mut tag, "DiscNumber", "2");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    refused(&path, &["TXXX:DiscNumber"]);
}

#[test]
fn legacy_txxx_equal_to_the_shown_value_goes_silently() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm(&mut tag, "Stejny");
    txxx(&mut tag, "Comment", "Stejny");
    text_frame(&mut tag, "TPOS", "2/3");
    txxx(&mut tag, "DiscNumber", "2");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &retitled(&path), &MUSIC);

    let raw = raw_id3(&path);
    assert!(txxx_descriptions(&raw).is_empty());
    assert_eq!(raw.comments().count(), 1);
}

#[test]
fn an_audiobiblio_file_with_only_legacy_txxx_becomes_comm_and_tpos() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "Comment", "Dlouhy popis knihy");
    txxx(&mut tag, "DiscNumber", "2");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &retitled(&path), &MUSIC);

    let raw = raw_id3(&path);
    let comments: Vec<_> = raw.comments().map(|c| c.content.to_string()).collect();
    assert_eq!(comments, strings(&["Dlouhy popis knihy"]));
    assert_eq!(raw.get_text(&lofty::id3::v2::FrameId::Valid("TPOS".into())), Some("2"));
    assert!(txxx_descriptions(&raw).is_empty());
}

#[test]
fn a_comm_in_another_language_with_other_text_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm_in(&mut tag, *b"eng", "Prvni");
    comm_in(&mut tag, *b"ces", "Druhy");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    refused(&path, &["COMM:ces"]);
}

#[test]
fn a_tyer_beside_tdrc_with_another_year_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TDRC", "2001");
    text_frame(&mut tag, "TYER", "1999");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    refused(&path, &["TYER"]);
}

#[test]
fn a_second_value_in_a_single_value_id3_frame_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TALB", "Kniha\0Jina kniha");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);
    assert_eq!(read(&path).fields[&TagField::Album], strings(&["Kniha"]));

    refused(&path, &["TALB"]);
}

#[test]
fn a_repeated_vorbis_single_value_key_refuses_unless_equal() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with(dir.path(), &[("ALBUM", "Kniha"), ("ALBUM", "Jina kniha")]);
    refused(&path, &["ALBUM#2"]);

    let same = tempfile::tempdir().unwrap();
    let path = flac_with(same.path(), &[("ALBUM", "Kniha"), ("album", "Kniha")]);
    write(&path, &retitled(&path), &MUSIC);
    assert_eq!(vorbis(&path), strings(&["ALBUM=Kniha", "TITLE=Novy nazev"]));
}

#[test]
fn an_m4a_gen_beside_a_differing_freeform_genre_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9gen", "Rock");
    freeform(&mut ilst, "com.apple.iTunes", "GENRE", &["Pop"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    refused(&path, &["\u{a9}gen"]);

    write(&path, &retitled(&path), &LOSSY);
    assert!(fourcc(&raw_ilst(&path), *b"\xA9gen").is_empty());
    assert_eq!(freeform_data(&raw_ilst(&path), "GENRE"), vec![&utf8("Pop")]);
}

#[test]
fn an_m4a_gen_equal_to_the_freeform_genre_goes_silently() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9gen", "audiokniha; thriller");
    freeform(&mut ilst, "com.apple.iTunes", "GENRE", &["audiokniha", "thriller"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    write(&path, &retitled(&path), &MUSIC);

    let raw = raw_ilst(&path);
    assert!(fourcc(&raw, *b"\xA9gen").is_empty());
    assert_eq!(
        freeform_data(&raw, "GENRE"),
        vec![&utf8("audiokniha"), &utf8("thriller")]
    );
}

/// An iTunes-tagged file has only `©gen`: the write moves its genre to the freeform GENRE.
#[test]
fn an_m4a_with_only_gen_moves_the_genre_to_freeform() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9gen", "audiokniha; thriller");
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    write(&path, &retitled(&path), &MUSIC);

    let raw = raw_ilst(&path);
    assert!(fourcc(&raw, *b"\xA9gen").is_empty());
    assert_eq!(
        freeform_data(&raw, "GENRE"),
        vec![&utf8("audiokniha"), &utf8("thriller")]
    );
    assert_eq!(
        read(&path).fields[&TagField::Genre],
        strings(&["audiokniha", "thriller"])
    );
}

#[test]
fn hidden_values_and_v23_losses_are_named_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm(&mut tag, "Ripped by X");
    txxx(&mut tag, "Comment", "Dlouhy popis knihy");
    text_frame(&mut tag, "TSOP", "Novak, Jan");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    refused(&path, &["TSOP", "TXXX:Comment"]);
}

#[test]
fn vorbis_album_artist_with_a_space_is_the_album_artist_and_goes_on_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with(dir.path(), &[("ALBUM ARTIST", "Karel Capek")]);

    let tags = read(&path);
    assert_eq!(tags.fields[&TagField::AlbumArtist], strings(&["Karel Capek"]));
    assert!(tags.extras.is_empty(), "{:?}", tags.extras);

    write(&path, &tags, &MUSIC);

    assert_eq!(vorbis(&path), strings(&["ALBUMARTIST=Karel Capek"]));
    assert_eq!(read(&path), tags);
}

#[test]
fn vorbis_multi_value_fields_split_a_joined_value() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with(
        dir.path(),
        &[
            ("GENRE", "audiokniha; thriller"),
            ("ARTIST", "A; B"),
            ("TITLE", "Jedna; dve"),
        ],
    );

    let tags = read(&path);
    assert_eq!(tags.fields[&TagField::Genre], strings(&["audiokniha", "thriller"]));
    assert_eq!(tags.fields[&TagField::Artist], strings(&["A", "B"]));
    assert_eq!(
        tags.fields[&TagField::Title],
        strings(&["Jedna; dve"]),
        "single-value fields stay whole"
    );

    write(&path, &tags, &MUSIC);

    let items = vorbis(&path);
    for item in ["GENRE=audiokniha", "GENRE=thriller", "ARTIST=A", "ARTIST=B"] {
        assert!(items.contains(&item.to_owned()), "{items:?}");
    }
    assert_eq!(read(&path), tags);
}

/// A multi-value field lists each value once, however many items carry it: an alias beside
/// its key, a repeated key, a joined value repeating another.
#[test]
fn a_multi_value_field_gathered_from_several_items_lists_each_value_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with(
        dir.path(),
        &[
            ("ALBUMARTIST", "X"),
            ("ALBUM ARTIST", "X"),
            ("GENRE", "audiokniha"),
            ("GENRE", "thriller; audiokniha"),
            ("GENRE", "thriller"),
        ],
    );

    let tags = read(&path);
    assert_eq!(tags.fields[&TagField::AlbumArtist], strings(&["X"]));
    assert_eq!(tags.fields[&TagField::Genre], strings(&["audiokniha", "thriller"]));

    write(&path, &tags, &MUSIC);

    assert_eq!(
        vorbis(&path),
        strings(&["ALBUMARTIST=X", "GENRE=audiokniha", "GENRE=thriller"])
    );
    assert_eq!(read(&path), tags);
}

#[test]
fn an_id3_frame_repeating_a_value_shows_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TPE1", "A\0B\0A");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    assert_eq!(read(&path).fields[&TagField::Artist], strings(&["A", "B"]));
}

/// The caller may hand the writer a value twice; the file gets it once, and the read-back
/// check accepts that.
#[test]
fn a_value_given_twice_is_written_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with(dir.path(), &[("TITLE", "Kniha")]);
    let mut tags = read(&path);
    tags.fields
        .insert(TagField::Genre, strings(&["rock", "pop; rock", "rock"]));

    write(&path, &tags, &MUSIC);

    assert_eq!(vorbis(&path), strings(&["TITLE=Kniha", "GENRE=rock", "GENRE=pop"]));
}
