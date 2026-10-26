//! `write_tags` round trips: write, then read back with `read_tags` and with lofty directly.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use cmdr_media_meta::test_fixtures::{cover, flac_stream, mp3_frames};
use lofty::TextEncoding;
use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{
    AttachedPictureFrame, Frame, FrameId, Id3v2Tag, KeyValueFrame, PrivateFrame, RelativeVolumeAdjustmentFrame,
    TimestampFrame,
};
use lofty::mp4::{Atom, AtomData, AtomIdent, DataType, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::items::Timestamp;

use crate::model::{ContentKind, Cover, FileTags, TagError, TagField};
use crate::read::read_tags;
use crate::test_files::*;
use crate::write::{WriteOptions, write_tags};

pub(crate) const MUSIC: WriteOptions = WriteOptions {
    kind: ContentKind::Music,
    track_total: None,
    allow_frame_loss: false,
};
const BOOK: WriteOptions = WriteOptions {
    kind: ContentKind::Audiobook,
    ..MUSIC
};

pub(crate) fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

pub(crate) fn read(path: &Path) -> FileTags {
    read_tags(path).expect("the file reads")
}

pub(crate) fn write(path: &Path, tags: &FileTags, opts: &WriteOptions) {
    write_tags(path, tags, opts).expect("the write succeeds");
}

fn with(mut tags: FileTags, field: TagField, values: &[&str]) -> FileTags {
    tags.fields.insert(field, strings(values));
    tags
}

fn without(mut tags: FileTags, field: TagField) -> FileTags {
    tags.fields.remove(&field);
    tags
}

/// One value for every field, Artist with two.
pub(crate) fn every_field() -> FileTags {
    let values: [(TagField, &[&str]); 15] = [
        (TagField::Title, &["Kapitola 1"]),
        (TagField::Artist, &["A", "B"]),
        (TagField::AlbumArtist, &["A"]),
        (TagField::Album, &["Kniha"]),
        (TagField::Performer, &["Jan Novotny"]),
        (TagField::Composer, &["Skladatel"]),
        (TagField::Date, &["1965"]),
        (TagField::Genre, &["audiokniha", "thriller"]),
        (TagField::Publisher, &["Argo"]),
        (TagField::Translator, &["Jan Pekar"]),
        (TagField::Track, &["3"]),
        (TagField::Disc, &["2"]),
        (TagField::Comment, &["Povídky"]),
        (TagField::Description, &["Popis knihy"]),
        (TagField::Www, &["https://example.org"]),
    ];
    let fields: BTreeMap<TagField, Vec<String>> = values.iter().map(|(f, v)| (*f, strings(v))).collect();
    FileTags {
        fields,
        ..FileTags::default()
    }
}

pub(crate) fn jpeg(len: usize, byte: u8) -> Cover {
    Cover {
        mime: "image/jpeg".to_owned(),
        data: vec![byte; len],
    }
}

fn parse_options() -> ParseOptions {
    ParseOptions::new().read_properties(false)
}

fn raw_id3(path: &Path) -> Id3v2Tag {
    let file = MpegFile::read_from(&mut File::open(path).unwrap(), parse_options()).unwrap();
    file.id3v2().cloned().unwrap_or_default()
}

fn raw_ilst(path: &Path) -> Ilst {
    let file = Mp4File::read_from(&mut File::open(path).unwrap(), parse_options()).unwrap();
    file.ilst().cloned().unwrap_or_default()
}

fn raw_flac(path: &Path) -> FlacFile {
    FlacFile::read_from(&mut File::open(path).unwrap(), parse_options()).unwrap()
}

fn id3_text<'a>(tag: &'a Id3v2Tag, id: &'static str) -> Option<&'a str> {
    tag.get_text(&FrameId::Valid(id.into()))
}

fn id3_has(tag: &Id3v2Tag, id: &str) -> bool {
    tag.into_iter().any(|f| f.id_str() == id)
}

fn id3_pictures(tag: &Id3v2Tag) -> Vec<&Picture> {
    tag.into_iter()
        .filter_map(|f| match f {
            Frame::Picture(p) => Some(p.picture.as_ref()),
            _ => None,
        })
        .collect()
}

fn txxx_descriptions(tag: &Id3v2Tag) -> Vec<String> {
    tag.into_iter()
        .filter_map(|f| match f {
            Frame::UserText(t) => Some(t.description.to_string()),
            _ => None,
        })
        .collect()
}

fn freeform_data<'a>(ilst: &'a Ilst, name: &str) -> Vec<&'a AtomData> {
    ilst.into_iter()
        .filter(|a| matches!(a.ident(), AtomIdent::Freeform { name: n, .. } if n == name))
        .flat_map(|a| a.data())
        .collect()
}

fn fourcc(ilst: &Ilst, code: [u8; 4]) -> Vec<&AtomData> {
    ilst.get(&AtomIdent::Fourcc(code))
        .map(|a| a.data().collect())
        .unwrap_or_default()
}

fn utf8(value: &str) -> AtomData {
    AtomData::UTF8(value.to_owned())
}

pub(crate) fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The bytes after an ID3v2 tag: the MPEG frames.
fn after_id3(bytes: &[u8]) -> &[u8] {
    assert_eq!(&bytes[..3], b"ID3");
    let size = bytes[6..10].iter().fold(0usize, |acc, b| (acc << 7) | usize::from(*b));
    &bytes[10 + size..]
}

/// The bytes after a FLAC stream's metadata blocks: the audio frames.
fn after_flac_metadata(bytes: &[u8]) -> &[u8] {
    assert_eq!(&bytes[..4], b"fLaC");
    let mut pos = 4;
    loop {
        let header = &bytes[pos..pos + 4];
        let len = usize::from(header[1]) << 16 | usize::from(header[2]) << 8 | usize::from(header[3]);
        pos += 4 + len;
        if header[0] & 0x80 != 0 {
            return &bytes[pos..];
        }
    }
}

// ── MP3 ───────────────────────────────────────────────────────────────────────

#[test]
fn mp3_round_trip_all_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");

    write(&path, &every_field(), &MUSIC);

    assert_eq!(read(&path), every_field());
    assert_eq!(fs::read(&path).unwrap()[3], 3, "written as ID3v2.3");
    let raw = raw_id3(&path);
    assert_eq!(id3_text(&raw, "TPE1"), Some("A; B"));
    assert_eq!(id3_text(&raw, "TCON"), Some("audiokniha; thriller"));
    let mut descriptions = txxx_descriptions(&raw);
    descriptions.sort();
    assert_eq!(
        descriptions,
        strings(&["Description", "Performer", "Translator", "www"])
    );
}

#[test]
fn mp3_writes_comm_and_drops_legacy_txxx() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "Comment", "Stary komentar");
    txxx(&mut tag, "DiscNumber", "2");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &read(&path), &MUSIC);

    let raw = raw_id3(&path);
    let comments: Vec<_> = raw.comments().map(|c| (c.language, c.content.to_string())).collect();
    assert_eq!(comments, vec![(*b"eng", "Stary komentar".to_owned())]);
    assert_eq!(id3_text(&raw, "TPOS"), Some("2"));
    assert!(txxx_descriptions(&raw).is_empty(), "{:?}", txxx_descriptions(&raw));
}

#[test]
fn mp3_managed_txxx_is_matched_case_insensitively() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "performer", "Stary ctenar");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &with(read(&path), TagField::Performer, &["Novy ctenar"]), &MUSIC);

    assert_eq!(txxx_descriptions(&raw_id3(&path)), strings(&["Performer"]));
    assert_eq!(read(&path).fields[&TagField::Performer], strings(&["Novy ctenar"]));
}

#[test]
fn mp3_duplicate_single_value_frames_collapse_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    comm(&mut tag, "Prvni");
    comm_in(&mut tag, *b"ces", "Druhy");
    text_frame(&mut tag, "TDRC", "2001");
    text_frame(&mut tag, "TYER", "1999");
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    write(&path, &read(&path), &MUSIC);

    let raw = raw_id3(&path);
    assert_eq!(raw.comments().count(), 1);
    let bytes = fs::read(&path).unwrap();
    let count = |id: &[u8]| bytes.windows(4).filter(|w| *w == id).count();
    assert_eq!((count(b"TYER"), count(b"TDRC")), (1, 0));
    assert_eq!(read(&path).fields[&TagField::Date], strings(&["2001"]));
}

#[test]
fn audiobook_removes_composer() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TCOM", "Skladatel");
    text_frame(&mut tag, "TPE3", "Dirigent");
    let mp3 = write_mp3(dir.path(), "a.mp3", &tag);
    let mut comments = VorbisComments::new();
    comments.push("COMPOSER".to_owned(), "Skladatel".to_owned());
    let flac = write_flac(dir.path(), "a.flac", comments, 0);
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9wrt", "Skladatel");
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);

    for path in [&mp3, &flac, &m4a] {
        write(path, &read(path), &BOOK);
        assert!(!read(path).fields.contains_key(&TagField::Composer), "{path:?}");
    }

    let raw = raw_id3(&mp3);
    assert!(!id3_has(&raw, "TCOM") && !id3_has(&raw, "TPE3"));
    assert!(
        raw_flac(&flac)
            .vorbis_comments()
            .is_none_or(|c| c.get("COMPOSER").is_none())
    );
    assert!(fourcc(&raw_ilst(&m4a), *b"\xA9wrt").is_empty());
}

#[test]
fn music_keeps_composer() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TCOM", "Skladatel");
    text_frame(&mut tag, "TPE3", "Dirigent");
    let mp3 = write_mp3(dir.path(), "a.mp3", &tag);
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9wrt", "Skladatel");
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);

    for path in [&mp3, &m4a] {
        write(path, &read(path), &MUSIC);
        assert_eq!(
            read(path).fields[&TagField::Composer],
            strings(&["Skladatel"]),
            "{path:?}"
        );
    }
    assert_eq!(id3_text(&raw_id3(&mp3), "TPE3"), Some("Dirigent"));
}

#[test]
fn empty_field_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", "Nazev");
    text_frame(&mut tag, "TALB", "Album");
    txxx(&mut tag, "Performer", "Ctenar");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let tags = with(without(read(&path), TagField::Title), TagField::Album, &["  ", ""]);
    write(&path, &with(tags, TagField::Performer, &[]), &MUSIC);

    let raw = raw_id3(&path);
    assert!(!id3_has(&raw, "TIT2") && !id3_has(&raw, "TALB") && !id3_has(&raw, "TXXX"));
    assert!(read(&path).fields.is_empty());
}

#[test]
fn track_total_written_only_when_given() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let tags = with(FileTags::default(), TagField::Track, &["3"]);

    write(
        &path,
        &tags,
        &WriteOptions {
            track_total: Some(12),
            ..MUSIC
        },
    );
    assert_eq!(id3_text(&raw_id3(&path), "TRCK"), Some("3/12"));

    write(&path, &tags, &MUSIC);
    assert_eq!(id3_text(&raw_id3(&path), "TRCK"), Some("3"));
}

#[test]
fn unmanaged_frames_survive() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(Frame::Private(PrivateFrame::new("cmdr.test", vec![1u8, 2, 3])));
    id3_cover(&mut tag, 40);
    id3_picture(&mut tag, 50, PictureType::CoverBack);
    text_frame(&mut tag, "TENC", "Encoder");
    txxx(&mut tag, "CUSTOM", "Hodnota");
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &with(read(&path), TagField::Title, &["Novy"]), &MUSIC);

    let raw = raw_id3(&path);
    let private = (&raw).into_iter().find_map(|f| match f {
        Frame::Private(p) => Some((p.owner.to_string(), p.private_data.to_vec())),
        _ => None,
    });
    assert_eq!(private, Some(("cmdr.test".to_owned(), vec![1, 2, 3])));
    let pictures: Vec<_> = id3_pictures(&raw)
        .iter()
        .map(|p| (p.pic_type(), p.data().len()))
        .collect();
    assert!(pictures.contains(&(PictureType::CoverBack, 50)), "{pictures:?}");
    assert!(pictures.contains(&(PictureType::CoverFront, 40)), "{pictures:?}");
    assert_eq!(pictures.len(), 2);
    assert_eq!(id3_text(&raw, "TENC"), Some("Encoder"));
    assert_eq!(read(&path).extras.get("CUSTOM"), Some(&strings(&["Hodnota"])));
}

#[test]
fn cover_replaced_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_cover(&mut tag, 40);
    id3_picture(&mut tag, 50, PictureType::CoverBack);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    let tags = FileTags {
        cover: Some(jpeg(60, 0xAB)),
        ..read(&path)
    };
    write(&path, &tags, &MUSIC);
    assert_eq!(read(&path).cover, Some(jpeg(60, 0xAB)));
    let pictures: Vec<_> = id3_pictures(&raw_id3(&path))
        .iter()
        .map(|p| (p.pic_type(), p.data().len()))
        .collect();
    assert_eq!(pictures.len(), 2, "{pictures:?}");
    assert!(pictures.contains(&(PictureType::CoverFront, 60)) && pictures.contains(&(PictureType::CoverBack, 50)));

    write(&path, &FileTags { cover: None, ..tags }, &MUSIC);
    let pictures: Vec<_> = id3_pictures(&raw_id3(&path))
        .iter()
        .map(|p| (p.pic_type(), p.data().len()))
        .collect();
    assert_eq!(pictures, vec![(PictureType::CoverBack, 50)]);
}

#[test]
fn id3_fallback_picture_is_the_cover_and_is_never_doubled() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    id3_picture(&mut tag, 77, PictureType::Other);
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &read(&path), &MUSIC);
    let pictures = id3_pictures(&raw_id3(&path)).len();
    assert_eq!(pictures, 1);

    write(
        &path,
        &FileTags {
            cover: None,
            ..read(&path)
        },
        &MUSIC,
    );
    assert!(id3_pictures(&raw_id3(&path)).is_empty());
}

#[test]
fn unparseable_mp3_date_is_an_error_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();

    let result = write_tags(
        &path,
        &with(FileTags::default(), TagField::Date, &["kolem roku"]),
        &MUSIC,
    );

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn mp3_audio_payload_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", "Kratky");
    let path = write_mp3(dir.path(), "a.mp3", &tag);
    let before = fs::read(&path).unwrap();
    assert_eq!(after_id3(&before), mp3_frames().as_slice());

    let tags = FileTags {
        cover: Some(jpeg(5000, 0x11)),
        ..every_field()
    };
    write(&path, &tags, &MUSIC);

    assert_eq!(after_id3(&fs::read(&path).unwrap()), after_id3(&before));
}

/// An ID3v2.4 tag with `TSOP` (performer sort) and `RVA2` (volume adjustment), frames
/// ID3v2.3 has no place for.
fn mp3_with_v24_only_frames(dir: &Path) -> PathBuf {
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", "Nazev");
    text_frame(&mut tag, "TSOP", "Novotny, Jan");
    tag.insert(Frame::RelativeVolumeAdjustment(RelativeVolumeAdjustmentFrame::new(
        "normalize",
        Cow::Owned(HashMap::new()),
    )));
    write_mp3_v24(dir, "a.mp3", &tag)
}

#[test]
fn mp3_write_that_would_drop_v24_frames_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = mp3_with_v24_only_frames(dir.path());
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    let result = write_tags(&path, &read(&path), &MUSIC);

    assert_eq!(result, Err(TagError::WouldDrop(strings(&["RVA2", "TSOP"]))));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

#[test]
fn mp3_frame_loss_allowed_writes_without_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = mp3_with_v24_only_frames(dir.path());
    let opts = WriteOptions {
        allow_frame_loss: true,
        ..MUSIC
    };

    write(&path, &with(read(&path), TagField::Title, &["Novy"]), &opts);

    let raw = raw_id3(&path);
    assert!(!id3_has(&raw, "TSOP") && !id3_has(&raw, "RVA2"));
    assert_eq!(id3_text(&raw, "TIT2"), Some("Novy"));
    assert_eq!(fs::read(&path).unwrap()[3], 3, "written as ID3v2.3");
}

#[test]
fn unchanged_cover_keeps_its_type_and_description() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    let picture = Picture::unchecked(vec![0xC1; 30])
        .pic_type(PictureType::Other)
        .mime_type(MimeType::Jpeg)
        .description("predni strana")
        .build();
    tag.insert(Frame::Picture(AttachedPictureFrame::new(TextEncoding::UTF8, picture)));
    let path = write_mp3(dir.path(), "a.mp3", &tag);

    write(&path, &with(read(&path), TagField::Title, &["Novy"]), &MUSIC);

    let raw = raw_id3(&path);
    let pictures = id3_pictures(&raw);
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].pic_type(), PictureType::Other);
    assert_eq!(pictures[0].description(), Some("predni strana"));
}

#[test]
fn value_that_reads_back_differently_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    // ID3 separates values with NUL, so this title would read back as just "A".
    let result = write_tags(&path, &with(FileTags::default(), TagField::Title, &["A\0B"]), &MUSIC);

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

fn tdor(year: u16, month: Option<u8>, day: Option<u8>) -> Frame<'static> {
    let timestamp = Timestamp {
        year,
        month,
        day,
        ..Timestamp::default()
    };
    Frame::Timestamp(TimestampFrame::new(
        FrameId::Valid(Cow::Borrowed("TDOR")),
        TextEncoding::UTF8,
        timestamp,
    ))
}

#[test]
fn mp3_full_original_date_would_be_cut_to_its_year() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(tdor(2001, Some(5), Some(3)));
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);
    let before = fs::read(&path).unwrap();

    let result = write_tags(&path, &read(&path), &MUSIC);

    assert_eq!(result, Err(TagError::WouldDrop(strings(&["TDOR"]))));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn mp3_original_year_alone_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(tdor(2001, None, None));
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    write(&path, &read(&path), &MUSIC);

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.windows(4).any(|w| w == b"TORY"), "ID3v2.3 keeps the year as TORY");
}

fn key_value(id: &'static str, pairs: &[(&'static str, &'static str)]) -> Frame<'static> {
    let pairs: Vec<(Cow<'static, str>, Cow<'static, str>)> = pairs
        .iter()
        .map(|(k, v)| (Cow::Borrowed(*k), Cow::Borrowed(*v)))
        .collect();
    Frame::KeyValue(KeyValueFrame::new(
        FrameId::Valid(Cow::Borrowed(id)),
        TextEncoding::UTF8,
        pairs,
    ))
}

#[test]
fn mp3_musician_credits_would_merge_into_involved_people() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(key_value("TIPL", &[("producer", "Jan")]));
    tag.insert(key_value("TMCL", &[("piano", "Eva")]));
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    let result = write_tags(&path, &read(&path), &MUSIC);

    assert_eq!(result, Err(TagError::WouldDrop(strings(&["TMCL"]))));
}

#[test]
fn mp3_involved_people_alone_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(key_value("TIPL", &[("producer", "Jan"), ("mix", "Eva")]));
    let path = write_mp3_v24(dir.path(), "a.mp3", &tag);

    write(&path, &read(&path), &MUSIC);

    let raw = raw_id3(&path);
    let pairs: Option<Vec<(String, String)>> = (&raw).into_iter().find_map(|f| match f {
        Frame::KeyValue(kv) if f.id_str() == "TIPL" => Some(
            kv.key_value_pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ),
        _ => None,
    });
    let expected = [("producer", "Jan"), ("mix", "Eva")].map(|(k, v)| (k.to_owned(), v.to_owned()));
    assert_eq!(pairs, Some(expected.to_vec()), "TIPL comes back as TIPL");
}

/// lofty's strict parse doesn't look for an ID3v2 tag behind junk bytes; writing from it
/// would put a second tag in front of the first.
#[test]
fn mp3_tag_behind_junk_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut tag = Id3v2Tag::new();
    text_frame(&mut tag, "TIT2", "Nazev");
    let tagged = fs::read(write_mp3(dir.path(), "tagged.mp3", &tag)).unwrap();
    let path = write_bytes(dir.path(), "a.mp3", [b"junk".as_slice(), &tagged].concat());
    assert_eq!(read(&path).fields[&TagField::Title], strings(&["Nazev"]));
    let before = fs::read(&path).unwrap();

    let result = write_tags(&path, &with(read(&path), TagField::Title, &["Novy"]), &MUSIC);

    assert!(matches!(result, Err(TagError::Parse(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
}

// ── M4A ───────────────────────────────────────────────────────────────────────

#[test]
fn m4a_round_trip_all_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    let tags = FileTags {
        cover: Some(jpeg(90, 0x22)),
        ..every_field()
    };

    write(
        &path,
        &tags,
        &WriteOptions {
            track_total: Some(12),
            ..MUSIC
        },
    );

    assert_eq!(read(&path), tags);
    let raw = raw_ilst(&path);
    assert_eq!(fourcc(&raw, *b"\xA9ART"), vec![&utf8("A; B")]);
    assert_eq!(freeform_data(&raw, "PERFORMER"), vec![&utf8("Jan Novotny")]);
    let trkn = fourcc(&raw, *b"trkn");
    assert!(
        matches!(trkn.as_slice(), [AtomData::Unknown { data, .. }] if data[2..6] == [0, 3, 0, 12]),
        "{trkn:?}"
    );
}

#[test]
fn m4a_writes_gen_and_freeform_genre() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    fourcc_text(&mut ilst, *b"\xA9gen", "stary");
    freeform(&mut ilst, "com.apple.iTunes", "genre", &["stary"]);
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    write(
        &path,
        &with(read(&path), TagField::Genre, &["audiokniha", "thriller"]),
        &MUSIC,
    );

    let raw = raw_ilst(&path);
    assert_eq!(fourcc(&raw, *b"\xA9gen"), vec![&utf8("audiokniha; thriller")]);
    assert_eq!(
        freeform_data(&raw, "GENRE"),
        vec![&utf8("audiokniha"), &utf8("thriller")]
    );
    assert!(
        freeform_data(&raw, "genre").is_empty(),
        "the old lowercase atom is gone"
    );
    assert_eq!(
        read(&path).fields[&TagField::Genre],
        strings(&["audiokniha", "thriller"])
    );
}

#[test]
fn extras_absent_from_tags_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    freeform(&mut ilst, "com.apple.iTunes", "ITUNESMEDIATYPE", &["Audiobook"]);
    freeform(&mut ilst, "com.apple.iTunes", "KEEP", &["ano"]);
    let m4a = write_m4a(dir.path(), "a.m4a", &ilst);
    let mut comments = VorbisComments::new();
    comments.push("ITUNESMEDIATYPE".to_owned(), "Audiobook".to_owned());
    comments.push("keep".to_owned(), "ano".to_owned());
    let flac = write_flac(dir.path(), "a.flac", comments, 0);
    let mut tag = Id3v2Tag::new();
    txxx(&mut tag, "ITUNESMEDIATYPE", "Audiobook");
    txxx(&mut tag, "KEEP", "ano");
    let mp3 = write_mp3(dir.path(), "a.mp3", &tag);

    for path in [&m4a, &flac, &mp3] {
        let mut tags = read(path);
        assert_eq!(tags.extras.len(), 2, "{path:?}: {:?}", tags.extras);
        tags.extras.remove("ITUNESMEDIATYPE");
        write(path, &tags, &MUSIC);
        let extras: Vec<_> = read(path).extras.into_keys().collect();
        assert_eq!(extras, strings(&["KEEP"]), "{path:?}");
    }
    assert!(freeform_data(&raw_ilst(&m4a), "ITUNESMEDIATYPE").is_empty());
}

#[test]
fn m4a_cover_replaced_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    ilst.insert_picture(cover(40));
    ilst.insert_picture(cover(50));
    let path = write_m4a(dir.path(), "a.m4a", &ilst);

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(60, 0x33)),
            ..read(&path)
        },
        &MUSIC,
    );
    let sizes: Vec<_> = raw_ilst(&path).pictures().unwrap().map(|p| p.data().len()).collect();
    assert_eq!(sizes, vec![60, 50]);

    write(
        &path,
        &FileTags {
            cover: None,
            ..read(&path)
        },
        &MUSIC,
    );
    let sizes: Vec<_> = raw_ilst(&path).pictures().unwrap().map(|p| p.data().len()).collect();
    assert_eq!(sizes, vec![50]);
}

#[test]
fn unsupported_m4a_cover_type_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_m4a(dir.path(), "a.m4a", &Ilst::new());
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());
    let webp = Cover {
        mime: "image/webp".to_owned(),
        data: vec![1; 10],
    };

    let result = write_tags(
        &path,
        &FileTags {
            cover: Some(webp),
            ..every_field()
        },
        &MUSIC,
    );

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

/// lofty 0.25.4 truncates a FLAC whose metadata runs to the end of the file when the
/// metadata shrinks; the read-back check refuses the broken copy.
#[test]
fn corrupting_save_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    comments.push("COMPOSER".to_owned(), "Skladatel".to_owned());
    let path = write_vorbis_stream(dir.path(), "a.flac", flac_stream(), comments, 0);
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    let result = write_tags(&path, &read(&path), &BOOK);

    assert!(matches!(result, Err(TagError::Write(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

/// lofty drops a whole `covr` atom it can't fully decode when reading leniently; writing from
/// that reading would delete the cover, so the write refuses instead.
#[test]
fn strict_parse_failure_refuses_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut ilst = Ilst::new();
    let covr = vec![
        AtomData::Picture(cover(40)),
        AtomData::Unknown {
            code: DataType::Other(99),
            data: vec![1, 2, 3],
        },
    ];
    ilst.insert(Atom::from_collection(AtomIdent::Fourcc(*b"covr"), covr).unwrap());
    let path = write_m4a(dir.path(), "a.m4a", &ilst);
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());
    assert_eq!(read(&path).cover, None, "the lenient reader drops the atom");

    let result = write_tags(&path, &with(read(&path), TagField::Title, &["Novy"]), &MUSIC);

    assert!(matches!(result, Err(TagError::Parse(_))), "{result:?}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

#[test]
fn m4a_chunk_offsets_follow_a_growing_tag() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(dir.path(), "a.m4b", m4a_with_chunks(3));
    let chunk_at = |bytes: &[u8], offset: usize| bytes[offset..offset + CHUNK_LEN].to_vec();
    let before = fs::read(&path).unwrap();
    for (i, offset) in stco_offsets(&before).into_iter().enumerate() {
        assert_eq!(chunk_at(&before, offset), vec![u8::try_from(i + 1).unwrap(); CHUNK_LEN]);
    }

    let tags = FileTags {
        cover: Some(jpeg(20_000, 0x12)),
        ..every_field()
    };
    write(&path, &tags, &MUSIC);

    let after = fs::read(&path).unwrap();
    assert!(after.len() > before.len() + 20_000);
    let offsets = stco_offsets(&after);
    assert_eq!(offsets.len(), 3);
    for (i, offset) in offsets.into_iter().enumerate() {
        assert_eq!(
            chunk_at(&after, offset),
            vec![u8::try_from(i + 1).unwrap(); CHUNK_LEN],
            "chunk {i}"
        );
    }
    assert_eq!(read(&path), tags);
}

// ── FLAC, Ogg, Opus ───────────────────────────────────────────────────────────

#[test]
fn flac_writes_repeated_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    let tags = FileTags {
        cover: Some(jpeg(70, 0x44)),
        ..every_field()
    };

    write(&path, &tags, &MUSIC);

    assert_eq!(read(&path), tags);
    let raw = raw_flac(&path);
    let comments = raw.vorbis_comments().expect("a comment block");
    assert_eq!(comments.get_all("ARTIST").collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(
        comments.get_all("GENRE").collect::<Vec<_>>(),
        ["audiokniha", "thriller"]
    );
    assert_eq!(raw.pictures().len(), 1);
}

#[test]
fn flac_cover_replaced_keeps_other_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    let mut back = cover(50);
    back.set_pic_type(PictureType::CoverBack);
    comments.insert_picture(back, None).unwrap();
    let path = write_flac(dir.path(), "a.flac", comments, 40);

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(60, 0x55)),
            ..read(&path)
        },
        &MUSIC,
    );

    let pictures: Vec<_> = raw_flac(&path)
        .pictures()
        .iter()
        .map(|(p, _)| (p.pic_type(), p.data().len()))
        .collect();
    assert_eq!(pictures.len(), 2, "{pictures:?}");
    assert!(pictures.contains(&(PictureType::CoverFront, 60)) && pictures.contains(&(PictureType::CoverBack, 50)));
}

#[test]
fn flac_audio_payload_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 30);
    let frames = flac_frames();
    assert_eq!(after_flac_metadata(&fs::read(&path).unwrap()), frames.as_slice());

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(4000, 0x66)),
            ..every_field()
        },
        &MUSIC,
    );

    assert_eq!(after_flac_metadata(&fs::read(&path).unwrap()), frames.as_slice());
}

#[test]
fn flac_track_total_written_only_when_given() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_flac(dir.path(), "a.flac", VorbisComments::new(), 0);
    let tags = with(FileTags::default(), TagField::Track, &["3"]);
    let comments = |path: &Path| raw_flac(path).vorbis_comments().cloned().unwrap_or_default();

    write(
        &path,
        &tags,
        &WriteOptions {
            track_total: Some(12),
            ..MUSIC
        },
    );
    let raw = comments(&path);
    assert_eq!(raw.get_all("TRACKNUMBER").collect::<Vec<_>>(), ["3"]);
    assert_eq!(raw.get_all("TRACKTOTAL").collect::<Vec<_>>(), ["12"]);
    assert_eq!(read(&path), tags, "the total is no extra");

    write(&path, &tags, &MUSIC);
    let raw = comments(&path);
    assert_eq!(raw.get_all("TRACKNUMBER").collect::<Vec<_>>(), ["3"]);
    assert_eq!(raw.get("TRACKTOTAL"), None);
}

#[test]
fn flac_write_without_total_removes_old_totals() {
    let dir = tempfile::tempdir().unwrap();
    let mut comments = VorbisComments::new();
    for (key, value) in [
        ("TRACKNUMBER", "3/12"),
        ("TRACKTOTAL", "12"),
        ("TOTALTRACKS", "12"),
        ("DISCTOTAL", "2"),
        ("TOTALDISCS", "2"),
    ] {
        comments.push(key.to_owned(), value.to_owned());
    }
    let path = write_flac(dir.path(), "a.flac", comments, 0);
    let tags = read(&path);
    assert_eq!(tags, with(FileTags::default(), TagField::Track, &["3"]));

    write(&path, &tags, &MUSIC);

    let raw = raw_flac(&path).vorbis_comments().cloned().unwrap_or_default();
    let keys: Vec<_> = raw.items().map(|(key, value)| format!("{key}={value}")).collect();
    assert_eq!(keys, ["TRACKNUMBER=3"]);
}

/// A FLAC with a back cover as a picture block and the front cover only inside the comments.
fn flac_with_cover_in_comments(dir: &Path) -> PathBuf {
    let mut back = cover(50);
    back.set_pic_type(PictureType::CoverBack);
    let front = cover(40);
    let bytes = flac_with_blocks(&[
        (FLAC_PICTURE, picture_block(&back)),
        (FLAC_VORBIS_COMMENT, vorbis_comment_body(&[picture_comment(&front)])),
    ]);
    write_bytes(dir, "a.flac", bytes)
}

fn flac_picture_types(path: &Path) -> Vec<(PictureType, usize)> {
    let file = raw_flac(path);
    let in_comments = file
        .vorbis_comments()
        .map(|c| c.pictures().to_vec())
        .unwrap_or_default();
    file.pictures()
        .iter()
        .chain(&in_comments)
        .map(|(p, _)| (p.pic_type(), p.data().len()))
        .collect()
}

#[test]
fn flac_cover_in_comments_is_the_cover_and_is_never_doubled() {
    let dir = tempfile::tempdir().unwrap();
    let path = flac_with_cover_in_comments(dir.path());
    assert_eq!(read(&path).cover.map(|c| c.data.len()), Some(40));

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(60, 0x88)),
            ..read(&path)
        },
        &MUSIC,
    );
    let mut pictures = flac_picture_types(&path);
    pictures.sort_by_key(|(_, len)| *len);
    assert_eq!(pictures, [(PictureType::CoverBack, 50), (PictureType::CoverFront, 60)]);

    write(
        &path,
        &FileTags {
            cover: None,
            ..read(&path)
        },
        &MUSIC,
    );
    assert_eq!(flac_picture_types(&path), [(PictureType::CoverBack, 50)]);
}

#[test]
fn flac_unmanaged_blocks_survive() {
    let dir = tempfile::tempdir().unwrap();
    let seek_point = [0u8; 18];
    let application = [b"cmdr".as_slice(), &[7u8; 12]].concat();
    let mut cuesheet = vec![0u8; 396];
    cuesheet.extend([0u8; 36]); // one lead-out track, number 170
    cuesheet[396 + 8] = 170;
    let blocks = [
        (FLAC_SEEKTABLE, seek_point.to_vec()),
        (FLAC_APPLICATION, application.clone()),
        (FLAC_CUESHEET, cuesheet.clone()),
        (FLAC_VORBIS_COMMENT, vorbis_comment_body(&["TITLE=Stary".to_owned()])),
    ];
    let path = write_bytes(dir.path(), "a.flac", flac_with_blocks(&blocks));

    write(
        &path,
        &FileTags {
            cover: Some(jpeg(3000, 0x99)),
            ..every_field()
        },
        &MUSIC,
    );

    let bytes = fs::read(&path).unwrap();
    let body_of = |kind: u8| {
        flac_block_bodies(&bytes)
            .into_iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, b)| b)
    };
    assert_eq!(body_of(FLAC_SEEKTABLE), Some(seek_point.to_vec()));
    assert_eq!(body_of(FLAC_APPLICATION), Some(application));
    assert_eq!(body_of(FLAC_CUESHEET), Some(cuesheet));
    assert_eq!(after_flac_metadata(&bytes), flac_frames().as_slice());
}

/// Every metadata block of a FLAC's bytes, as (type, body).
fn flac_block_bodies(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut blocks = Vec::new();
    let mut pos = 4;
    loop {
        let header = &bytes[pos..pos + 4];
        let len = usize::from(header[1]) << 16 | usize::from(header[2]) << 8 | usize::from(header[3]);
        blocks.push((header[0] & 0x7F, bytes[pos + 4..pos + 4 + len].to_vec()));
        pos += 4 + len;
        if header[0] & 0x80 != 0 {
            return blocks;
        }
    }
}

/// A FLAC holds one icon of each icon type; lofty's `insert_picture` drops a second one
/// when the writer gathers the pictures.
#[test]
fn flac_second_icon_would_be_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let icon = |len: usize| {
        let mut picture = cover(len);
        picture.set_pic_type(PictureType::Icon);
        picture_block(&picture)
    };
    let bytes = flac_with_blocks(&[(FLAC_PICTURE, icon(20)), (FLAC_PICTURE, icon(30))]);
    let path = write_bytes(dir.path(), "a.flac", bytes);
    let before = fs::read(&path).unwrap();

    let result = write_tags(&path, &with(read(&path), TagField::Title, &["Novy"]), &MUSIC);

    assert_eq!(result, Err(TagError::WouldDrop(strings(&["PICTURE:Icon"]))));
    assert_eq!(fs::read(&path).unwrap(), before);
}

fn ogg_family_round_trip(path: &Path) {
    let tags = FileTags {
        cover: Some(jpeg(80, 0x77)),
        ..every_field()
    };

    write(path, &tags, &MUSIC);

    assert_eq!(read(path), tags);
}

#[test]
fn ogg_vorbis_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_ogg_vorbis(dir.path(), "a.ogg", VorbisComments::new(), 30);
    ogg_family_round_trip(&path);
    let file = VorbisFile::read_from(&mut File::open(&path).unwrap(), parse_options()).unwrap();
    assert_eq!(file.vorbis_comments().get_all("ARTIST").collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(file.vorbis_comments().pictures().len(), 1);
}

#[test]
fn opus_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_opus(dir.path(), "a.opus", VorbisComments::new(), 30);
    ogg_family_round_trip(&path);
    let file = OpusFile::read_from(&mut File::open(&path).unwrap(), parse_options()).unwrap();
    assert_eq!(
        file.vorbis_comments().get_all("GENRE").collect::<Vec<_>>(),
        ["audiokniha", "thriller"]
    );
    assert_eq!(file.vorbis_comments().pictures().len(), 1);
}

#[test]
fn unsupported_format_is_not_supported() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(dir.path(), "a.wav", vec![0; 10]);
    assert_eq!(write_tags(&path, &every_field(), &MUSIC), Err(TagError::NotSupported));
}
