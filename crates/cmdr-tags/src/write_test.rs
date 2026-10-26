//! `write_tags` round trips: write, then read back with `read_tags` and with lofty directly.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use cmdr_media_meta::test_fixtures::{cover, flac_stream, mp3_frames};
use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, PrivateFrame};
use lofty::mp4::{AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{Picture, PictureType};

use crate::model::{ContentKind, Cover, FileTags, TagError, TagField};
use crate::read::read_tags;
use crate::test_files::*;
use crate::write::{WriteOptions, save_safely, write_tags};

const MUSIC: WriteOptions = WriteOptions {
    kind: ContentKind::Music,
    track_total: None,
};
const BOOK: WriteOptions = WriteOptions {
    kind: ContentKind::Audiobook,
    track_total: None,
};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

fn read(path: &Path) -> FileTags {
    read_tags(path).expect("the file reads")
}

fn write(path: &Path, tags: &FileTags, opts: &WriteOptions) {
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
fn every_field() -> FileTags {
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

fn jpeg(len: usize, byte: u8) -> Cover {
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

fn listing(dir: &Path) -> Vec<String> {
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

#[test]
fn failed_save_leaves_original_and_no_temp() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());

    let result = save_safely(&path, |out| {
        out.write_all(b"half a tag")?;
        Err(TagError::Write("stopped partway".to_owned()))
    });

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

fn ogg_family_round_trip(path: &Path) {
    let tags = FileTags {
        cover: Some(jpeg(80, 0x77)),
        ..every_field()
    };
    let mut expected = tags.clone();
    expected.extras = read(path).extras;

    write(path, &tags, &MUSIC);

    assert_eq!(
        read(path),
        FileTags {
            extras: BTreeMap::new(),
            ..expected.clone()
        }
    );
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

// ── Safe overwrite ────────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn failed_write_leaves_original_and_no_temp() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let before = fs::read(&path).unwrap();
    let names = listing(dir.path());
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = write_tags(&path, &every_field(), &MUSIC);

    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(TagError::Write(_) | TagError::Io(..))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(listing(dir.path()), names);
}

#[cfg(unix)]
#[test]
fn write_keeps_permissions_and_leaves_no_temp() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    write(&path, &every_field(), &MUSIC);

    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(listing(dir.path()), strings(&["a.mp3"]));
}

#[cfg(unix)]
#[test]
fn read_only_file_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    let before = fs::read(&path).unwrap();

    let result = write_tags(&path, &every_field(), &MUSIC);

    assert!(
        matches!(result, Err(TagError::Io(std::io::ErrorKind::PermissionDenied, _))),
        "{result:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn write_through_a_symlink_keeps_the_link() {
    let dir = tempfile::tempdir().unwrap();
    let target = write_untagged_mp3(dir.path(), "a.mp3");
    let link: PathBuf = dir.path().join("link.mp3");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    write(&link, &every_field(), &MUSIC);

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(read(&target), every_field());
}

#[cfg(target_os = "macos")]
#[test]
fn write_keeps_extended_attributes() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = write_untagged_mp3(dir.path(), "a.mp3");
    let status = Command::new("xattr")
        .args(["-w", "com.cmdr.test", "hodnota"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());

    write(&path, &every_field(), &MUSIC);

    let out = Command::new("xattr")
        .args(["-p", "com.cmdr.test"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hodnota");
}

#[test]
fn unsupported_format_is_not_supported() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bytes(dir.path(), "a.wav", vec![0; 10]);
    assert_eq!(write_tags(&path, &every_field(), &MUSIC), Err(TagError::NotSupported));
}
