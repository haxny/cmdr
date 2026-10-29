//! The cover-art pre-pass: `APIC` and `covr` mapped to holes exactly, lofty
//! reading the tags around them without their bytes, and what's left unmapped.

use std::io::{self, Cursor};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use lofty::config::WriteOptions;
use lofty::tag::TagType;

use crate::holes::cover_art;
use crate::read::{read_media, read_media_from};
use crate::source::ByteSource;
use crate::test_fixtures::{CountingReader, concat, cover, flac_stream, m4a_file, mp3_frames, music_tag, tagged};

const COVER_LEN: usize = 3 * 1024 * 1024;

fn mp3_with_cover(options: WriteOptions) -> Vec<u8> {
    let mut tag = music_tag(TagType::Id3v2);
    tag.push_picture(cover(COVER_LEN));
    tagged(mp3_frames(), &tag, options)
}

fn m4a_with_cover() -> Vec<u8> {
    let mut tag = music_tag(TagType::Mp4Ilst);
    tag.push_picture(cover(COVER_LEN));
    tagged(m4a_file(), &tag, WriteOptions::default())
}

/// The one hole in `holes`, checked to cover the picture's bytes.
fn the_cover_hole(bytes: &[u8], holes: &[Range<u64>]) -> Range<usize> {
    assert_eq!(holes.len(), 1, "{holes:?}");
    let hole = holes[0].start as usize..holes[0].end as usize;
    let picture = bytes[hole.clone()].iter().filter(|b| **b == 0xC0).count();
    assert!(picture >= COVER_LEN, "the hole holds {picture} picture bytes");
    assert!(
        hole.len() < COVER_LEN + 64,
        "the hole is {} bytes, past the frame",
        hole.len()
    );
    hole
}

#[test]
fn an_apic_body_is_a_hole_in_id3v2_4_and_2_3() {
    for options in [WriteOptions::default(), WriteOptions::default().use_id3v23(true)] {
        let bytes = mp3_with_cover(options);
        let holes = cover_art(&mut Cursor::new(bytes.clone()), "song.mp3").unwrap();
        the_cover_hole(&bytes, &holes);
    }
}

#[test]
fn lofty_reads_the_tags_around_an_apic_hole_without_its_bytes() {
    let bytes = mp3_with_cover(WriteOptions::default());
    let holes = cover_art(&mut Cursor::new(bytes.clone()), "song.mp3").unwrap();
    let (reader, read) = CountingReader::new(bytes);
    let facts = read_media(&mut reader.with_holes(holes), "song.mp3").unwrap();
    assert_eq!(facts.title.as_deref(), Some("Digital Love"));
    assert_eq!(facts.track, Some(3));
    assert!(read.get() < 64 * 1024, "read {} bytes besides the cover", read.get());
}

#[test]
fn a_covr_picture_is_a_hole_and_lofty_reads_around_it() {
    let bytes = m4a_with_cover();
    let holes = cover_art(&mut Cursor::new(bytes.clone()), "song.m4a").unwrap();
    the_cover_hole(&bytes, &holes);
    let (reader, read) = CountingReader::new(bytes);
    let facts = read_media(&mut reader.with_holes(holes), "song.m4a").unwrap();
    assert_eq!(facts.album.as_deref(), Some("Discovery"));
    assert!(read.get() < 64 * 1024, "read {} bytes besides the cover", read.get());
}

#[test]
fn a_flac_picture_needs_no_hole_lofty_seeks_past_it() {
    let mut tag = music_tag(TagType::VorbisComments);
    tag.push_picture(cover(COVER_LEN));
    let bytes = tagged(flac_stream(), &tag, WriteOptions::default());
    assert_eq!(
        cover_art(&mut Cursor::new(bytes.clone()), "song.flac").unwrap(),
        Vec::new()
    );
    let (mut reader, read) = CountingReader::new(bytes);
    assert_eq!(
        read_media(&mut reader, "song.flac").unwrap().title.as_deref(),
        Some("Digital Love")
    );
    assert!(read.get() < 64 * 1024, "read {} bytes", read.get());
}

#[test]
fn tags_the_pre_pass_cant_map_exactly_get_no_holes() {
    let bytes = mp3_with_cover(WriteOptions::default());
    let with_flags = |flags: u8| {
        let mut b = bytes.clone();
        b[5] = flags;
        b
    };
    let mut v22 = bytes.clone();
    v22[3] = 2;
    for (what, b) in [
        ("unsynchronised", with_flags(0x80)),
        ("extended header", with_flags(0x40)),
        ("ID3v2.2", v22),
    ] {
        assert_eq!(
            cover_art(&mut Cursor::new(b), "song.mp3").unwrap(),
            Vec::new(),
            "{what}"
        );
    }
}

#[test]
fn hostile_frames_end_the_walk() {
    // A frame claiming 4 GB inside a 100-byte tag, then truncations of a real one.
    let huge = concat(&[
        b"ID3\x03\x00\x00\x00\x00\x00\x64",
        b"APIC",
        &u32::MAX.to_be_bytes(),
        &[0; 2],
        &[0; 90],
    ]);
    let holes = cover_art(&mut Cursor::new(huge), "a.mp3").unwrap();
    assert!(holes.iter().all(|h| h.end <= 110), "{holes:?}");
    let whole = mp3_with_cover(WriteOptions::default());
    for len in (0..400).chain([whole.len() / 2]) {
        cover_art(&mut Cursor::new(whole[..len].to_vec()), "a.mp3").unwrap();
    }
    let m4a = m4a_with_cover();
    for len in (0..m4a.len()).step_by(997).take(200) {
        cover_art(&mut Cursor::new(m4a[..len].to_vec()), "a.m4a").unwrap();
    }
}

/// A byte source over memory that records every fetch.
struct RecordingSource {
    bytes: Vec<u8>,
    fetched: Arc<Mutex<Vec<Range<u64>>>>,
}

impl ByteSource for RecordingSource {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let start = (offset as usize).min(self.bytes.len());
        let end = start.saturating_add(len).min(self.bytes.len());
        self.fetched
            .lock()
            .expect("the test's own lock")
            .push(offset..end as u64);
        Ok(self.bytes[start..end].to_vec())
    }
}

#[test]
fn through_the_block_reader_a_big_cover_is_never_fetched() {
    for (name, bytes) in [
        ("song.mp3", mp3_with_cover(WriteOptions::default())),
        ("song.m4a", m4a_with_cover()),
    ] {
        let fetched = Arc::new(Mutex::new(Vec::new()));
        let source = RecordingSource {
            bytes,
            fetched: Arc::clone(&fetched),
        };
        let facts = read_media_from(source, name).unwrap();
        assert_eq!(facts.artist, vec!["Daft Punk"], "{name}");
        let total: u64 = fetched.lock().unwrap().iter().map(|r| r.end - r.start).sum();
        assert!(total <= 4 * 64 * 1024, "{name}: fetched bytes: {total}");
    }
}
