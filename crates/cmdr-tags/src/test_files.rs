//! Tagged audio files for the tests, written to a directory with lofty's
//! concrete tag types so the reader sees real bytes.

use std::borrow::Cow;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use cmdr_media_meta::test_fixtures::{cover, flac_stream, m4a_file, mp3_frames};
use lofty::TextEncoding;
use lofty::config::WriteOptions;
use lofty::id3::v2::{CommentFrame, ExtendedTextFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame};
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst};
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{PictureInformation, PictureType};
use lofty::tag::TagExt;

/// Adds a text frame such as `TIT2`.
pub fn text_frame(tag: &mut Id3v2Tag, id: &'static str, value: &str) {
    let frame = TextInformationFrame::new(FrameId::Valid(Cow::Borrowed(id)), TextEncoding::UTF8, value.to_owned());
    tag.insert(Frame::Text(frame));
}

/// Adds a `TXXX` frame.
pub fn txxx(tag: &mut Id3v2Tag, description: &str, value: &str) {
    tag.insert(Frame::UserText(ExtendedTextFrame::new(
        TextEncoding::UTF8,
        description.to_owned(),
        value.to_owned(),
    )));
}

/// Adds a `COMM` frame with an empty description.
pub fn comm(tag: &mut Id3v2Tag, value: &str) {
    comm_in(tag, *b"eng", value);
}

/// Adds a `COMM` frame with an empty description in `language`.
pub fn comm_in(tag: &mut Id3v2Tag, language: [u8; 3], value: &str) {
    tag.insert(Frame::Comment(CommentFrame::new(
        TextEncoding::UTF8,
        language,
        String::new(),
        value.to_owned(),
    )));
}

/// Adds a front cover of `len` bytes.
pub fn id3_cover(tag: &mut Id3v2Tag, len: usize) {
    tag.insert(Frame::Picture(lofty::id3::v2::AttachedPictureFrame::new(
        TextEncoding::UTF8,
        cover(len),
    )));
}

/// Adds a picture of `len` bytes with the given type.
pub fn id3_picture(tag: &mut Id3v2Tag, len: usize, kind: PictureType) {
    let mut picture = cover(len);
    picture.set_pic_type(kind);
    tag.insert(Frame::Picture(lofty::id3::v2::AttachedPictureFrame::new(
        TextEncoding::UTF8,
        picture,
    )));
}

/// Writes `tag` as a real ID3v2.4 tag.
pub fn write_mp3_v24(dir: &Path, name: &str, tag: &Id3v2Tag) -> PathBuf {
    let mut cursor = Cursor::new(mp3_frames());
    tag.save_to(&mut cursor, WriteOptions::new().use_id3v23(false))
        .expect("lofty writes the ID3v2 tag");
    write_bytes(dir, name, cursor.into_inner())
}

/// Writes `tag` as ID3v2.3 into a fresh MP3 named `name` under `dir`.
pub fn write_mp3(dir: &Path, name: &str, tag: &Id3v2Tag) -> PathBuf {
    let mut cursor = Cursor::new(mp3_frames());
    tag.save_to(&mut cursor, WriteOptions::new().use_id3v23(true))
        .expect("lofty writes the ID3v2 tag");
    write_bytes(dir, name, cursor.into_inner())
}

/// An MP3 with no tag at all.
pub fn write_untagged_mp3(dir: &Path, name: &str) -> PathBuf {
    write_bytes(dir, name, mp3_frames())
}

/// Adds a freeform atom with one data entry per value.
pub fn freeform(ilst: &mut Ilst, mean: &str, name: &str, values: &[&str]) {
    let ident = AtomIdent::Freeform {
        mean: Cow::Owned(mean.to_owned()),
        name: Cow::Owned(name.to_owned()),
    };
    let data = values.iter().map(|v| AtomData::UTF8((*v).to_owned())).collect();
    ilst.insert(Atom::from_collection(ident, data).expect("at least one value"));
}

/// Adds a text atom such as `©gen`.
pub fn fourcc_text(ilst: &mut Ilst, fourcc: [u8; 4], value: &str) {
    ilst.insert(Atom::new(AtomIdent::Fourcc(fourcc), AtomData::UTF8(value.to_owned())));
}

/// Writes `ilst` into a fresh M4A named `name` under `dir`.
pub fn write_m4a(dir: &Path, name: &str, ilst: &Ilst) -> PathBuf {
    let mut cursor = Cursor::new(m4a_file());
    ilst.save_to(&mut cursor, WriteOptions::default())
        .expect("lofty writes the ilst");
    write_bytes(dir, name, cursor.into_inner())
}

/// Writes `comments` into a fresh FLAC named `name` under `dir`, with a front cover of
/// `cover_len` bytes when it is non-zero. Stand-in audio frames follow the metadata, as in
/// every real FLAC.
pub fn write_flac(dir: &Path, name: &str, comments: VorbisComments, cover_len: usize) -> PathBuf {
    let stream = [flac_stream(), flac_frames()].concat();
    write_vorbis_stream(dir, name, stream, comments, cover_len)
}

/// Bytes standing in for FLAC audio frames (lofty reads tags without parsing them).
pub fn flac_frames() -> Vec<u8> {
    (0..2000u32).map(|i| (i % 251) as u8).collect()
}

/// Writes `comments` into a fresh Ogg Vorbis file.
pub fn write_ogg_vorbis(dir: &Path, name: &str, comments: VorbisComments, cover_len: usize) -> PathBuf {
    write_vorbis_stream(dir, name, ogg_vorbis_stream(), comments, cover_len)
}

/// Writes `comments` into a fresh Opus file.
pub fn write_opus(dir: &Path, name: &str, comments: VorbisComments, cover_len: usize) -> PathBuf {
    write_vorbis_stream(dir, name, opus_stream(), comments, cover_len)
}

/// Saves `comments` (and a front cover of `cover_len` bytes when non-zero) into `stream`
/// (FLAC, Ogg Vorbis, or Opus) and writes it as `name` under `dir`.
pub fn write_vorbis_stream(
    dir: &Path,
    name: &str,
    stream: Vec<u8>,
    mut comments: VorbisComments,
    cover_len: usize,
) -> PathBuf {
    if cover_len > 0 {
        comments
            .insert_picture(cover(cover_len), Some(PictureInformation::default()))
            .expect("a picture lofty accepts");
    }
    let mut cursor = Cursor::new(stream);
    comments
        .save_to(&mut cursor, WriteOptions::default())
        .expect("lofty writes the Vorbis comments");
    write_bytes(dir, name, cursor.into_inner())
}

// ── Ogg ───────────────────────────────────────────────────────────────────────

/// Ogg's page checksum: CRC-32, polynomial 0x04C11DB7, no reflection, initial value 0.
fn ogg_crc(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |crc, byte| {
        (0..8).fold(crc ^ (u32::from(*byte) << 24), |c, _| {
            if c & 0x8000_0000 != 0 {
                (c << 1) ^ 0x04C1_1DB7
            } else {
                c << 1
            }
        })
    })
}

/// One Ogg page holding whole `packets`. `kind` is the header-type byte (2 = first page, 4 = last).
fn ogg_page(kind: u8, sequence: u32, granule: i64, packets: &[&[u8]]) -> Vec<u8> {
    let mut lacing = Vec::new();
    for packet in packets {
        lacing.extend(std::iter::repeat_n(255u8, packet.len() / 255));
        lacing.push((packet.len() % 255) as u8);
    }
    let mut page = b"OggS".to_vec();
    page.extend([0, kind]);
    page.extend(granule.to_le_bytes());
    page.extend(1u32.to_le_bytes()); // stream serial number
    page.extend(sequence.to_le_bytes());
    page.extend(0u32.to_le_bytes()); // checksum, filled in below
    page.push(u8::try_from(lacing.len()).expect("a small test page"));
    page.extend(&lacing);
    for packet in packets {
        page.extend(*packet);
    }
    let crc = ogg_crc(&page).to_le_bytes();
    page[22..26].copy_from_slice(&crc);
    page
}

/// A Vorbis comment block body: vendor, then no comments.
fn empty_comment_body() -> Vec<u8> {
    let mut body = 0u32.to_le_bytes().to_vec(); // vendor length
    body.extend(0u32.to_le_bytes()); // comment count
    body
}

/// A minimal Ogg Vorbis stream: identification, empty comments, and setup headers, then
/// one audio page. Enough for lofty to find and rewrite the comments.
pub fn ogg_vorbis_stream() -> Vec<u8> {
    let mut ident = b"\x01vorbis".to_vec();
    ident.extend(0u32.to_le_bytes()); // version
    ident.push(2); // channels
    ident.extend(44_100u32.to_le_bytes());
    ident.extend([0u8; 12]); // bitrates
    ident.extend([0xB8, 1]); // block sizes, framing
    let mut comments = b"\x03vorbis".to_vec();
    comments.extend(empty_comment_body());
    comments.push(1); // framing
    let setup = b"\x05vorbis\x00\x01";
    [
        ogg_page(2, 0, 0, &[&ident]),
        ogg_page(0, 1, 0, &[&comments, setup]),
        ogg_page(4, 2, 0, &[&[0u8; 8]]),
    ]
    .concat()
}

/// A minimal Opus stream: `OpusHead`, empty `OpusTags`, then one audio page.
pub fn opus_stream() -> Vec<u8> {
    let mut head = b"OpusHead".to_vec();
    head.extend([1, 2]); // version, channels
    head.extend(312u16.to_le_bytes()); // pre-skip
    head.extend(48_000u32.to_le_bytes());
    head.extend([0, 0, 0]); // gain, channel mapping family
    let mut tags = b"OpusTags".to_vec();
    tags.extend(empty_comment_body());
    [
        ogg_page(2, 0, 0, &[&head]),
        ogg_page(0, 1, 0, &[&tags]),
        ogg_page(4, 2, 960, &[&[0u8; 8]]),
    ]
    .concat()
}

/// A file of arbitrary bytes.
pub fn write_bytes(dir: &Path, name: &str, bytes: Vec<u8>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("the temp dir is writable");
    path
}
