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
    assert_eq!(facts.artist.as_deref(), Some("Daft Punk"));
    assert_eq!(facts.album.as_deref(), Some("Discovery"));
    assert_eq!(facts.album_artist.as_deref(), Some("Daft Punk"));
    assert_eq!(facts.genre.as_deref(), Some("House"));
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
