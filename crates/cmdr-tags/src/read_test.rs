//! `read_tags` against real files written with lofty's concrete tag types.

use std::path::Path;

use cmdr_media_meta::test_fixtures::mp3_frames;
use lofty::id3::v2::Id3v2Tag;
use lofty::mp4::Ilst;
use lofty::ogg::tag::VorbisComments;

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
fn reads_id3v24_null_separated_artists_as_two_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TPE1", "Serhij Rudenko\0Sergej Rudenko");
    text_frame(&mut tag, "TCON", "audiokniha; thriller");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

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
