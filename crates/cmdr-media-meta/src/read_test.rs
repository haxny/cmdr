//! `read_media`: dispatch by extension, tags through lofty (fixtures written by
//! lofty itself), videos' dates plus tags, and refusals coming back typed.

use std::io::{self, Cursor, Read, Seek, SeekFrom};

use lofty::config::WriteOptions;
use lofty::tag::TagType;

use crate::read::{read_media, read_media_in};
use crate::test_fixtures::{
    CountingReader, at, atom, concat, el, flac_stream, ftyp, info_with_date, m4a_file, mac_seconds, matroska_nanos,
    mp3_frames, music_tag, mvhd_v0, tagged, two_hours_ahead,
};
use crate::types::{MediaFacts, ReadError};

fn read(bytes: Vec<u8>, name: &str) -> Result<MediaFacts, ReadError> {
    read_media_in(&mut Cursor::new(bytes), name, &two_hours_ahead)
}

fn assert_music(facts: &MediaFacts) {
    assert_eq!(facts.title.as_deref(), Some("Digital Love"));
    assert_eq!(facts.artist, vec!["Daft Punk"]);
    assert_eq!(facts.album.as_deref(), Some("Discovery"));
    assert_eq!(facts.album_artist, vec!["Daft Punk"]);
    assert_eq!(facts.genre, vec!["House"]);
    assert_eq!(facts.comment.as_deref(), Some("Remastered"));
    assert_eq!((facts.track, facts.disc, facts.year), (Some(3), Some(1), Some(2001)));
    assert_eq!(facts.taken, None);
}

#[test]
fn an_mp3_gives_its_id3v2_tags() {
    for options in [WriteOptions::default(), WriteOptions::default().use_id3v23(true)] {
        let bytes = tagged(mp3_frames(), &music_tag(TagType::Id3v2), options);
        assert_music(&read(bytes, "Digital Love.MP3").unwrap());
    }
}

#[test]
fn a_flac_gives_its_vorbis_comments() {
    let bytes = tagged(
        flac_stream(),
        &music_tag(TagType::VorbisComments),
        WriteOptions::default(),
    );
    assert_music(&read(bytes, "track.flac").unwrap());
}

#[test]
fn an_m4a_gives_its_ilst() {
    let bytes = tagged(m4a_file(), &music_tag(TagType::Mp4Ilst), WriteOptions::default());
    assert_music(&read(bytes, "track.m4a").unwrap());
}

#[test]
fn an_mp3_without_tags_has_no_fields() {
    assert_eq!(read(mp3_frames(), "silence.mp3").unwrap(), MediaFacts::default());
}

#[test]
fn a_video_gives_its_date_and_its_tags() {
    let utc = at(2026, 7, 14, 6, 0, 0);
    let movie = concat(&[
        &ftyp(b"isom"),
        &atom(b"moov", &mvhd_v0(u32::try_from(mac_seconds(utc)).unwrap())),
        &atom(b"mdat", &[0; 32]),
    ]);
    let mut tag = lofty::tag::Tag::new(TagType::Mp4Ilst);
    lofty::tag::Accessor::set_title(&mut tag, "Holiday".to_owned());
    let facts = read(tagged(movie, &tag, WriteOptions::default()), "IMG_0001.MOV").unwrap();
    assert_eq!(facts.taken, Some(at(2026, 7, 14, 8, 0, 0)));
    assert_eq!(facts.title.as_deref(), Some("Holiday"));
}

#[test]
fn a_video_lofty_cant_parse_keeps_its_date() {
    // A QuickTime file with no `ftyp`, as old cameras write it.
    let utc = at(2010, 5, 1, 10, 0, 0);
    let movie = concat(&[
        &atom(b"wide", &[]),
        &atom(b"mdat", &[0; 16]),
        &atom(b"moov", &mvhd_v0(u32::try_from(mac_seconds(utc)).unwrap())),
    ]);
    assert_eq!(
        read(movie, "MVI_0001.mov").unwrap().taken,
        Some(at(2010, 5, 1, 12, 0, 0))
    );
}

#[test]
fn matroska_and_photos_dispatch_to_their_readers() {
    let utc = at(2026, 7, 14, 7, 0, 0);
    let mkv = concat(&[
        &el(0x1A45_DFA3, &el(0x4282, b"matroska")),
        &el(0x1853_8067, &info_with_date(matroska_nanos(utc))),
    ]);
    assert_eq!(read(mkv, "clip.webm").unwrap().taken, Some(at(2026, 7, 14, 9, 0, 0)));
    let png = concat(&[
        b"\x89PNG\r\n\x1a\n",
        &13u32.to_be_bytes(),
        b"IHDR",
        &640u32.to_be_bytes(),
        &480u32.to_be_bytes(),
    ]);
    assert_eq!(read(png, "shot.png").unwrap().width, Some(640));
}

#[test]
fn unread_formats_are_not_supported_without_reading_a_byte() {
    for name in ["photo.CR3", "photo.raf", "notes.txt", "no-extension", ".jpg"] {
        let (mut reader, read) = CountingReader::new(vec![0xFF; 64]);
        assert_eq!(read_media(&mut reader, name), Err(ReadError::NotSupported), "{name}");
        assert_eq!(read.get(), 0, "{name}");
    }
}

#[test]
fn garbage_audio_is_unreadable_not_empty() {
    assert!(matches!(read(vec![0x42; 4096], "song.flac"), Err(ReadError::Io(_))));
}

/// A reader that refuses every read with `refusal`, as a cancelled or
/// over-budget block reader does.
struct Refusing(ReadError, Cursor<Vec<u8>>);

impl Read for Refusing {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(self.0.clone().into_io())
    }
}

impl Seek for Refusing {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.1.seek(pos)
    }
}

/// A reader that refuses with `refusal` inside `refused` and answers elsewhere,
/// as a block reader whose budget runs out partway does.
struct RefusingIn {
    bytes: Cursor<Vec<u8>>,
    refused: std::ops::Range<u64>,
    refusal: ReadError,
}

impl Read for RefusingIn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pos = self.bytes.position();
        if self.refused.contains(&pos) {
            return Err(self.refusal.clone().into_io());
        }
        let room = if pos < self.refused.start {
            self.refused.start - pos
        } else {
            u64::MAX
        };
        let len = buf.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        self.bytes.read(&mut buf[..len])
    }
}

impl Seek for RefusingIn {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(pos)
    }
}

#[test]
fn a_video_whose_tags_run_past_the_budget_keeps_its_date() {
    let utc = at(2026, 7, 14, 6, 0, 0);
    let movie = concat(&[
        &ftyp(b"isom"),
        &atom(b"moov", &mvhd_v0(u32::try_from(mac_seconds(utc)).unwrap())),
        &atom(b"mdat", &[0; 32]),
    ]);
    let mut tag = lofty::tag::Tag::new(TagType::Mp4Ilst);
    lofty::tag::Accessor::set_title(&mut tag, "Holiday".to_owned());
    let bytes = tagged(movie, &tag, WriteOptions::default());
    // The box walk reads `udta`'s header; lofty's tag pass reads into its body.
    let udta = bytes.windows(4).position(|w| w == b"udta").unwrap();
    let udta_size = u32::from_be_bytes(bytes[udta - 4..udta].try_into().unwrap());
    let body = udta as u64 + 4;
    let refused = |refusal: ReadError| {
        let mut reader = RefusingIn {
            bytes: Cursor::new(bytes.clone()),
            refused: body..body + u64::from(udta_size) - 8,
            refusal,
        };
        read_media_in(&mut reader, "IMG_0001.MOV", &two_hours_ahead)
    };
    let facts = refused(ReadError::TooLarge).expect("the date the box walk found stands");
    assert_eq!(facts.taken, Some(at(2026, 7, 14, 8, 0, 0)));
    assert_eq!(facts.title, None);
    for refusal in [ReadError::Cancelled, ReadError::TimedOut] {
        assert_eq!(refused(refusal.clone()), Err(refusal));
    }
}

#[test]
fn a_refusal_comes_back_as_itself_from_every_reader() {
    for refusal in [ReadError::Cancelled, ReadError::TimedOut, ReadError::TooLarge] {
        for name in ["a.jpg", "a.mp3", "a.flac", "a.m4a", "a.mov", "a.mkv", "a.heic", "a.dng"] {
            let mut reader = Refusing(refusal.clone(), Cursor::new(vec![0; 4096]));
            assert_eq!(read_media(&mut reader, name), Err(refusal.clone()), "{name}");
        }
    }
}

// ── The tag fields beyond the generic ones, read as cmdr-tags reads them ──────

mod native_fields {
    use std::borrow::Cow;
    use std::io::Cursor;

    use lofty::TextEncoding;
    use lofty::config::WriteOptions;
    use lofty::id3::v2::{ExtendedTextFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame};
    use lofty::mp4::{Atom, AtomData, AtomIdent, DataType, Ilst};
    use lofty::ogg::tag::VorbisComments;
    use lofty::tag::TagExt;

    use super::read;
    use crate::test_fixtures::{flac_stream, m4a_file, mp3_frames};

    fn text_frame(tag: &mut Id3v2Tag, id: &'static str, value: &str) {
        let frame = TextInformationFrame::new(FrameId::Valid(Cow::Borrowed(id)), TextEncoding::UTF8, value.to_owned());
        tag.insert(Frame::Text(frame));
    }

    fn txxx(tag: &mut Id3v2Tag, description: &str, value: &str) {
        tag.insert(Frame::UserText(ExtendedTextFrame::new(
            TextEncoding::UTF8,
            description.to_owned(),
            value.to_owned(),
        )));
    }

    fn mp3(tag: &Id3v2Tag, v23: bool) -> Vec<u8> {
        let mut cursor = Cursor::new(mp3_frames());
        tag.save_to(&mut cursor, WriteOptions::new().use_id3v23(v23)).unwrap();
        cursor.into_inner()
    }

    fn freeform(ilst: &mut Ilst, name: &str, values: &[&str]) {
        let ident = AtomIdent::Freeform {
            mean: Cow::Borrowed("com.apple.iTunes"),
            name: Cow::Owned(name.to_owned()),
        };
        let data = values.iter().map(|v| AtomData::UTF8((*v).to_owned())).collect();
        ilst.insert(Atom::from_collection(ident, data).unwrap());
    }

    fn flac(comments: &VorbisComments) -> Vec<u8> {
        let mut cursor = Cursor::new([flac_stream(), vec![0u8; 64]].concat());
        comments.save_to(&mut cursor, WriteOptions::default()).unwrap();
        cursor.into_inner()
    }

    #[test]
    fn an_mp3_gives_txxx_performer_and_the_rest_the_way_cmdr_tags_does() {
        let mut tag = Id3v2Tag::new();
        txxx(&mut tag, "PERFORMER", "Jan Hartl");
        text_frame(&mut tag, "TPE3", "A Conductor");
        text_frame(&mut tag, "TCOM", "Dvorak");
        text_frame(&mut tag, "TPUB", "CRo3");
        text_frame(&mut tag, "TDRC", "2010-09-20");
        text_frame(&mut tag, "TRCK", "3/12");
        text_frame(&mut tag, "TPOS", "1/2");
        txxx(&mut tag, "Translator", "Pavel Eisner");
        txxx(&mut tag, "Description", "About it");
        txxx(&mut tag, "www", "https://example.cz");
        let facts = read(mp3(&tag, false), "a.mp3").unwrap();
        // TPE3 is the conductor, never the performer.
        assert_eq!(facts.performer, ["Jan Hartl"]);
        assert_eq!(facts.composer, ["Dvorak"]);
        assert_eq!(facts.publisher.as_deref(), Some("CRo3"));
        assert_eq!(facts.date.as_deref(), Some("2010-09-20"));
        assert_eq!((facts.track, facts.disc, facts.year), (Some(3), Some(1), Some(2010)));
        assert_eq!((facts.total_tracks, facts.total_discs), (Some(12), Some(2)));
        assert_eq!(facts.translator, ["Pavel Eisner"]);
        assert_eq!(facts.description.as_deref(), Some("About it"));
        assert_eq!(facts.www, ["https://example.cz"]);
    }

    #[test]
    fn an_id3v23_year_frame_is_the_date() {
        let mut tag = Id3v2Tag::new();
        text_frame(&mut tag, "TYER", "1999");
        let facts = read(mp3(&tag, true), "a.mp3").unwrap();
        assert_eq!(facts.date.as_deref(), Some("1999"));
    }

    #[test]
    fn a_plain_track_number_has_no_total() {
        let mut tag = Id3v2Tag::new();
        text_frame(&mut tag, "TRCK", "3");
        let facts = read(mp3(&tag, false), "a.mp3").unwrap();
        assert_eq!((facts.track, facts.total_tracks), (Some(3), None));
    }

    #[test]
    fn null_separated_composers_are_several_values() {
        let mut tag = Id3v2Tag::new();
        text_frame(&mut tag, "TCOM", "A\0B");
        let facts = read(mp3(&tag, false), "a.mp3").unwrap();
        assert_eq!(facts.composer, ["A", "B"]);
    }

    #[test]
    fn an_m4a_gives_its_freeform_performer_and_the_rest() {
        let mut ilst = Ilst::new();
        freeform(&mut ilst, "PERFORMER", &["Jan Hartl", "Eva Novak"]);
        freeform(&mut ilst, "publisher", &["CRo3"]);
        freeform(&mut ilst, "www", &["https://example.cz"]);
        freeform(&mut ilst, "translator", &["Pavel Eisner"]);
        freeform(&mut ilst, "description", &["About it"]);
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*b"\xA9wrt"),
            AtomData::UTF8("Dvorak".to_owned()),
        ));
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*b"\xA9day"),
            AtomData::UTF8("2010-09-20".to_owned()),
        ));
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*b"trkn"),
            AtomData::Unknown {
                code: DataType::Reserved,
                data: vec![0, 0, 0, 3, 0, 12, 0, 0],
            },
        ));
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*b"disk"),
            AtomData::Unknown {
                code: DataType::Reserved,
                data: vec![0, 0, 0, 1, 0, 2],
            },
        ));
        let mut cursor = Cursor::new(m4a_file());
        ilst.save_to(&mut cursor, WriteOptions::default()).unwrap();
        let facts = read(cursor.into_inner(), "a.m4a").unwrap();
        assert_eq!(facts.performer, ["Jan Hartl", "Eva Novak"]);
        assert_eq!(facts.publisher.as_deref(), Some("CRo3"));
        assert_eq!(facts.www, ["https://example.cz"]);
        assert_eq!(facts.translator, ["Pavel Eisner"]);
        assert_eq!(facts.description.as_deref(), Some("About it"));
        assert_eq!(facts.composer, ["Dvorak"]);
        assert_eq!(facts.date.as_deref(), Some("2010-09-20"));
        assert_eq!((facts.total_tracks, facts.total_discs), (Some(12), Some(2)));
    }

    #[test]
    fn a_flac_gives_its_vorbis_performer_and_the_rest() {
        let mut comments = VorbisComments::new();
        for (key, value) in [
            ("PERFORMER", "Jan Hartl"),
            ("PERFORMER", "Eva Novak"),
            ("PUBLISHER", "CRo3"),
            ("COMPOSER", "Dvorak"),
            ("DATE", "2010-09-20"),
            ("TRACKNUMBER", "3"),
            ("TRACKTOTAL", "12"),
            ("DISCNUMBER", "1/2"),
            ("TRANSLATOR", "Pavel Eisner"),
            ("DESCRIPTION", "About it"),
            ("WWW", "https://example.cz"),
        ] {
            comments.push(key.to_owned(), value.to_owned());
        }
        let facts = read(flac(&comments), "a.flac").unwrap();
        assert_eq!(facts.performer, ["Jan Hartl", "Eva Novak"]);
        assert_eq!(facts.publisher.as_deref(), Some("CRo3"));
        assert_eq!(facts.composer, ["Dvorak"]);
        assert_eq!(facts.date.as_deref(), Some("2010-09-20"));
        assert_eq!((facts.total_tracks, facts.total_discs), (Some(12), Some(2)));
        assert_eq!(facts.translator, ["Pavel Eisner"]);
        assert_eq!(facts.description.as_deref(), Some("About it"));
        assert_eq!(facts.www, ["https://example.cz"]);
    }

    #[test]
    fn single_value_fields_keep_a_semicolon_inside() {
        let mut tag = Id3v2Tag::new();
        txxx(&mut tag, "Description", "Part one; part two");
        text_frame(&mut tag, "TPUB", "Odeon; Supraphon");
        let facts = read(mp3(&tag, false), "a.mp3").unwrap();
        assert_eq!(facts.description.as_deref(), Some("Part one; part two"));
        assert_eq!(facts.publisher.as_deref(), Some("Odeon; Supraphon"));

        let mut ilst = Ilst::new();
        freeform(&mut ilst, "description", &["Part one; part two"]);
        freeform(&mut ilst, "publisher", &["Odeon; Supraphon"]);
        freeform(&mut ilst, "PERFORMER", &["A; B"]);
        let mut cursor = Cursor::new(m4a_file());
        ilst.save_to(&mut cursor, WriteOptions::default()).unwrap();
        let facts = read(cursor.into_inner(), "a.m4a").unwrap();
        assert_eq!(facts.description.as_deref(), Some("Part one; part two"));
        assert_eq!(facts.publisher.as_deref(), Some("Odeon; Supraphon"));
        assert_eq!(facts.performer, ["A", "B"]);

        let mut comments = VorbisComments::new();
        comments.push("DESCRIPTION".to_owned(), "Part one; part two".to_owned());
        comments.push("PUBLISHER".to_owned(), "Odeon; Supraphon".to_owned());
        let facts = read(flac(&comments), "a.flac").unwrap();
        assert_eq!(facts.description.as_deref(), Some("Part one; part two"));
        assert_eq!(facts.publisher.as_deref(), Some("Odeon; Supraphon"));
    }

    #[test]
    fn multi_value_fields_split_on_a_semicolon_in_every_format() {
        let mut tag = Id3v2Tag::new();
        txxx(&mut tag, "Performer", "A; B");
        txxx(&mut tag, "www", "https://a.cz; https://b.cz");
        let facts = read(mp3(&tag, false), "a.mp3").unwrap();
        assert_eq!(facts.performer, ["A", "B"]);
        assert_eq!(facts.www, ["https://a.cz", "https://b.cz"]);

        let mut comments = VorbisComments::new();
        comments.push("PERFORMER".to_owned(), "A; B".to_owned());
        comments.push("COMPOSER".to_owned(), "C; D".to_owned());
        comments.push("TRANSLATOR".to_owned(), "E; F".to_owned());
        let facts = read(flac(&comments), "a.flac").unwrap();
        assert_eq!(facts.performer, ["A", "B"]);
        assert_eq!(facts.composer, ["C", "D"]);
        assert_eq!(facts.translator, ["E", "F"]);
    }

    #[test]
    fn an_m4a_total_needs_its_number() {
        let mut ilst = Ilst::new();
        ilst.insert(Atom::new(
            AtomIdent::Fourcc(*b"trkn"),
            AtomData::Unknown {
                code: DataType::Reserved,
                data: vec![0, 0, 0, 0, 0, 12],
            },
        ));
        let mut cursor = Cursor::new(m4a_file());
        ilst.save_to(&mut cursor, WriteOptions::default()).unwrap();
        assert_eq!(read(cursor.into_inner(), "a.m4a").unwrap().total_tracks, None);
    }

    #[test]
    fn a_vorbis_total_needs_its_track_number() {
        let mut comments = VorbisComments::new();
        comments.push("TRACKTOTAL".to_owned(), "12".to_owned());
        assert_eq!(read(flac(&comments), "a.flac").unwrap().total_tracks, None);
    }

    #[test]
    fn a_file_without_these_tags_has_none() {
        let facts = read(mp3_frames(), "a.mp3").unwrap();
        assert!(facts.performer.is_empty() && facts.publisher.is_none() && facts.total_tracks.is_none());
    }
}
