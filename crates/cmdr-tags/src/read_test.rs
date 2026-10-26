//! `read_tags` against real files written with lofty's concrete tag types.

use std::path::Path;

use cmdr_media_meta::test_fixtures::cover;
use cmdr_media_meta::test_fixtures::mp3_frames;
use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::PictureType;
use lofty::tag::Accessor;

use crate::model::{FileTags, TagError, TagField};
use crate::read::read_tags;
use crate::test_files::*;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

fn field(tags: &FileTags, field: TagField) -> Vec<String> {
    tags.fields.get(&field).cloned().unwrap_or_default()
}

fn read(path: &Path) -> FileTags {
    read_tags(path).expect("the file reads")
}

#[test]
fn reads_id3v23_fields_and_txxx_performer() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", " Dune ");
    text_frame(&mut tag, "TPE1", "Frank Herbert");
    text_frame(&mut tag, "TPE2", "Frank Herbert");
    text_frame(&mut tag, "TALB", "Dune");
    text_frame(&mut tag, "TYER", "1965");
    text_frame(&mut tag, "TRCK", "3/12");
    text_frame(&mut tag, "TPOS", "2/3");
    text_frame(&mut tag, "TCON", "audiokniha");
    text_frame(&mut tag, "TPUB", "Argo");
    comm(&mut tag, "Skvely pribeh");
    txxx(&mut tag, "Performer", "Jan Novotny");
    txxx(&mut tag, "Translator", "Jan Pekar");
    txxx(&mut tag, "Description", "Popis knihy");
    txxx(&mut tag, "www", "https://example.org");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let tags = read(&path);

    assert_eq!(field(&tags, TagField::Title), strings(&["Dune"]));
    assert_eq!(field(&tags, TagField::Artist), strings(&["Frank Herbert"]));
    assert_eq!(field(&tags, TagField::AlbumArtist), strings(&["Frank Herbert"]));
    assert_eq!(field(&tags, TagField::Album), strings(&["Dune"]));
    assert_eq!(field(&tags, TagField::Date), strings(&["1965"]));
    assert_eq!(field(&tags, TagField::Track), strings(&["3"]));
    assert_eq!(field(&tags, TagField::Disc), strings(&["2"]));
    assert_eq!(field(&tags, TagField::Genre), strings(&["audiokniha"]));
    assert_eq!(field(&tags, TagField::Publisher), strings(&["Argo"]));
    assert_eq!(field(&tags, TagField::Comment), strings(&["Skvely pribeh"]));
    assert_eq!(field(&tags, TagField::Performer), strings(&["Jan Novotny"]));
    assert_eq!(field(&tags, TagField::Translator), strings(&["Jan Pekar"]));
    assert_eq!(field(&tags, TagField::Description), strings(&["Popis knihy"]));
    assert_eq!(field(&tags, TagField::Www), strings(&["https://example.org"]));
    assert!(
        tags.extras.is_empty(),
        "managed TXXX frames aren't extras: {:?}",
        tags.extras
    );
}

#[test]
fn reads_real_id3v24_null_separated_artists_as_two_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TPE1", "Serhij Rudenko\0Sergej Rudenko");
    text_frame(&mut tag, "TCON", "audiokniha; thriller");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);
    assert_eq!(
        std::fs::read(&path).unwrap()[3],
        4,
        "the file carries an ID3v2.4 header"
    );

    let tags = read(&path);

    assert_eq!(
        field(&tags, TagField::Artist),
        strings(&["Serhij Rudenko", "Sergej Rudenko"])
    );
    assert_eq!(field(&tags, TagField::Genre), strings(&["audiokniha", "thriller"]));
}

#[test]
fn reads_legacy_txxx_comment_and_discnumber() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "Comment", "stary komentar");
    txxx(&mut tag, "DiscNumber", "4/5");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let tags = read(&path);

    assert_eq!(field(&tags, TagField::Comment), strings(&["stary komentar"]));
    assert_eq!(field(&tags, TagField::Disc), strings(&["4"]));
    assert!(tags.extras.is_empty(), "{:?}", tags.extras);
}

#[test]
fn reads_m4a_freeform_genre_over_gen() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(
        &mut ilst,
        "com.apple.iTunes",
        "GENRE",
        &["audiokniha", "biografie a memoary"],
    );
    fourcc_text(&mut ilst, *b"\xA9gen", "x");
    fourcc_text(&mut ilst, *b"\xA9nam", "Titul");
    freeform(&mut ilst, "com.apple.iTunes", "PERFORMER", &["Vypravec"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    let tags = read(&path);

    assert_eq!(
        field(&tags, TagField::Genre),
        strings(&["audiokniha", "biografie a memoary"])
    );
    assert_eq!(field(&tags, TagField::Title), strings(&["Titul"]));
    assert_eq!(field(&tags, TagField::Performer), strings(&["Vypravec"]));
}

#[test]
fn reads_m4a_gen_when_no_freeform() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9gen", "audiokniha; thriller");
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    let tags = read(&path);

    assert_eq!(field(&tags, TagField::Genre), strings(&["audiokniha", "thriller"]));
}

#[test]
fn reads_flac_repeated_artist() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.push("ARTIST".to_owned(), "Serhij Rudenko".to_owned());
    comments.push("ARTIST".to_owned(), "Sergej Rudenko".to_owned());
    comments.push("TRACKNUMBER".to_owned(), "7/9".to_owned());
    comments.push("custom".to_owned(), "Hodnota".to_owned());
    let path = write_flac(dir.path(), "a.flac", comments, 0);

    let tags = read(&path);

    assert_eq!(
        field(&tags, TagField::Artist),
        strings(&["Serhij Rudenko", "Sergej Rudenko"])
    );
    assert_eq!(field(&tags, TagField::Track), strings(&["7"]));
    assert_eq!(tags.extras.get("CUSTOM"), Some(&strings(&["Hodnota"])));
}

#[test]
fn unmanaged_txxx_lands_in_extras() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "ITUNESMEDIATYPE", "Music");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let tags = read(&path);

    assert_eq!(tags.extras.get("ITUNESMEDIATYPE"), Some(&strings(&["Music"])));
}

#[test]
fn m4a_unmanaged_freeform_lands_in_extras() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(&mut ilst, "com.apple.iTunes", "SUBTITLE", &["Podtitul"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    assert_eq!(read(&path).extras.get("SUBTITLE"), Some(&strings(&["Podtitul"])));
}

#[test]
fn front_cover_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_cover(&mut tag, 520);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let cover = read(&path).cover.expect("a cover");

    assert_eq!(cover.mime, "image/jpeg");
    assert_eq!(cover.data.len(), 520);
}

#[test]
fn flac_front_cover_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 300);

    let cover = read(&path).cover.expect("a cover");

    assert_eq!((cover.mime.as_str(), cover.data.len()), ("image/jpeg", 300));
}

#[test]
fn untagged_mp3_reads_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");

    assert_eq!(read(&path), FileTags::default());
}

#[test]
fn wav_is_not_supported() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(dir.path(), "a.wav", mp3_frames());

    assert_eq!(read_tags(&path), Err(TagError::NotSupported));
}

#[test]
fn missing_file_is_an_io_error() {
    let dir = tempfile::tempdir().unwrap();

    let result = read_tags(&dir.path().join("nope.mp3"));

    assert!(
        matches!(result, Err(TagError::Io(std::io::ErrorKind::NotFound, _))),
        "{result:?}"
    );
}

#[test]
fn id3_cover_falls_back_to_the_first_picture() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_picture(&mut tag, 77, PictureType::Other);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let cover = read(&path).cover.expect("the only picture is the cover");

    assert_eq!(cover.data.len(), 77);
}

#[test]
fn id3_front_cover_beats_other_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_picture(&mut tag, 11, PictureType::Other);
    id3_picture(&mut tag, 22, PictureType::CoverFront);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    assert_eq!(read(&path).cover.expect("a cover").data.len(), 22);
}

#[test]
fn single_value_fields_keep_the_first_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm_in(&mut tag, *b"eng", "first");
    comm_in(&mut tag, *b"ces", "second");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    assert_eq!(field(&read(&path), TagField::Comment).len(), 1);
}

#[test]
fn reads_every_m4a_field_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9alb", "Kniha");
    fourcc_text(&mut ilst, *b"\xA9ART", "Autor");
    fourcc_text(&mut ilst, *b"aART", "Autor");
    fourcc_text(&mut ilst, *b"\xA9nam", "Kapitola");
    fourcc_text(&mut ilst, *b"\xA9day", "2020");
    fourcc_text(&mut ilst, *b"\xA9cmt", "Poznamka");
    ilst.set_track(3);
    ilst.set_track_total(12);
    ilst.set_disk(1);
    ilst.insert_picture(cover(90));
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    let tags = read(&path);

    let expected = [
        (TagField::Album, "Kniha"),
        (TagField::Artist, "Autor"),
        (TagField::AlbumArtist, "Autor"),
        (TagField::Title, "Kapitola"),
        (TagField::Date, "2020"),
        (TagField::Comment, "Poznamka"),
        (TagField::Track, "3"),
        (TagField::Disc, "1"),
    ];
    assert_eq!(tags.fields.len(), expected.len(), "{:?}", tags.fields);
    for (f, value) in expected {
        assert_eq!(field(&tags, f), strings(&[value]), "{f:?}");
    }
    let cover = tags.cover.expect("a cover");
    assert_eq!((cover.mime.as_str(), cover.data.len()), ("image/jpeg", 90));
    assert!(tags.extras.is_empty(), "{:?}", tags.extras);
}

fn ogg_comments() -> VorbisComments {
    let mut comments = VorbisComments::new();
    comments.push("ARTIST".to_owned(), "Serhij Rudenko".to_owned());
    comments.push("ARTIST".to_owned(), "Sergej Rudenko".to_owned());
    comments.push("custom".to_owned(), "Hodnota".to_owned());
    comments
}

fn assert_ogg_family(path: &Path) {
    let tags = read(path);
    assert_eq!(
        field(&tags, TagField::Artist),
        strings(&["Serhij Rudenko", "Sergej Rudenko"])
    );
    assert_eq!(tags.extras.get("CUSTOM"), Some(&strings(&["Hodnota"])));
    let cover = tags.cover.expect("a cover");
    assert_eq!((cover.mime.as_str(), cover.data.len()), ("image/jpeg", 250));
}

#[test]
fn reads_ogg_vorbis_artists_extras_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    assert_ogg_family(&write_ogg_vorbis(dir.path(), "a.ogg", ogg_comments(), 250));
}

#[test]
fn reads_opus_artists_extras_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    assert_ogg_family(&write_opus(dir.path(), "a.opus", ogg_comments(), 250));
}

#[test]
fn read_duration_of_a_real_file() {
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    // 13 x 3 frames of 1152 samples at 44.1 kHz.
    let frames = 39.0_f64;
    let path = write_bytes(dir.path(), "a.mp3", mp3_frames().repeat(13));
    let expected = Duration::from_secs_f64(frames * 1152.0 / 44100.0);
    let got = crate::read::read_duration(&path).expect("a duration");
    assert!(
        got.abs_diff(expected) <= Duration::from_millis(50),
        "{got:?} vs {expected:?}"
    );
}

#[test]
fn read_duration_none_for_garbage() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(dir.path(), "g.mp3", vec![0x42; 64]);
    assert_eq!(crate::read::read_duration(&path), None);
    for name in ["g.m4a", "g.m4b", "g.flac", "g.ogg", "g.opus"] {
        let junk = write_bytes(dir.path(), name, vec![0x42; 64]);
        assert_eq!(crate::read::read_duration(&junk), None, "{name}");
    }
    assert_eq!(crate::read::read_duration(&dir.path().join("missing.mp3")), None);
    let unknown = write_bytes(dir.path(), "g.txt", vec![1, 2, 3]);
    assert_eq!(crate::read::read_duration(&unknown), None);
}
