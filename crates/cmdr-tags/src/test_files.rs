//! Tagged audio files for the tests, written to a directory with lofty's
//! concrete tag types so the reader sees real bytes.

use std::borrow::Cow;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use cmdr_media_meta::test_fixtures::{atom, concat, cover, flac_stream, ftyp, full, m4a_file, mp3_frames, mvhd_v0};
use lofty::TextEncoding;
use lofty::config::WriteOptions;
use lofty::id3::v2::{CommentFrame, ExtendedTextFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame};
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst};
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{Picture, PictureInformation, PictureType};
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

// ── Hand-built FLAC blocks ────────────────────────────────────────────────────

/// FLAC metadata block types.
pub const FLAC_APPLICATION: u8 = 2;
/// See [`FLAC_APPLICATION`].
pub const FLAC_SEEKTABLE: u8 = 3;
/// See [`FLAC_APPLICATION`].
pub const FLAC_VORBIS_COMMENT: u8 = 4;
/// See [`FLAC_APPLICATION`].
pub const FLAC_CUESHEET: u8 = 5;
/// See [`FLAC_APPLICATION`].
pub const FLAC_PICTURE: u8 = 6;

/// A FLAC: STREAMINFO, then `blocks` (type, body) in order, then the stand-in audio frames.
/// lofty's writer never sees these bytes, so a test controls exactly where each block sits.
pub fn flac_with_blocks(blocks: &[(u8, Vec<u8>)]) -> Vec<u8> {
    assert!(!blocks.is_empty(), "STREAMINFO alone is `flac_stream`");
    let mut out = flac_stream();
    out[4] = 0; // STREAMINFO is no longer the last block
    for (i, (kind, body)) in blocks.iter().enumerate() {
        let last = if i + 1 == blocks.len() { 0x80 } else { 0 };
        let len = u32::try_from(body.len()).expect("a small test block").to_be_bytes();
        out.push(kind | last);
        out.extend(&len[1..]);
        out.extend(body);
    }
    out.extend(flac_frames());
    out
}

/// A Vorbis comment block body holding `entries` (`KEY=value`), without a framing bit.
pub fn vorbis_comment_body(entries: &[String]) -> Vec<u8> {
    let vendor = b"cmdr-test";
    let mut body = u32::try_from(vendor.len()).unwrap().to_le_bytes().to_vec();
    body.extend(vendor);
    body.extend(u32::try_from(entries.len()).unwrap().to_le_bytes());
    for entry in entries {
        body.extend(u32::try_from(entry.len()).unwrap().to_le_bytes());
        body.extend(entry.as_bytes());
    }
    body
}

/// `picture` as a `METADATA_BLOCK_PICTURE=...` comment entry (base64).
pub fn picture_comment(picture: &Picture) -> String {
    let encoded = picture.as_flac_bytes(PictureInformation::default(), true);
    format!(
        "METADATA_BLOCK_PICTURE={}",
        String::from_utf8(encoded).expect("base64 is ASCII")
    )
}

/// `picture` as a FLAC PICTURE block body.
pub fn picture_block(picture: &Picture) -> Vec<u8> {
    picture.as_flac_bytes(PictureInformation::default(), false)
}

// ── M4A with sample tables ────────────────────────────────────────────────────

/// Bytes per chunk in [`m4a_with_chunks`].
pub const CHUNK_LEN: usize = 64;

/// An M4A whose one track has a chunk offset table (`moov.trak.mdia.minf.stbl.stco`) pointing
/// at `chunks` chunks inside `mdat`, chunk `i` filled with byte `i + 1`, as in a real file
/// whose `moov` comes before `mdat`. Growing the tag moves `mdat`, so every offset must move.
pub fn m4a_with_chunks(chunks: u8) -> Vec<u8> {
    m4a_with_tracks(b"M4A ", &[(b"soun", chunks)])
}

/// An MP4 of brand `brand` with one `trak` per `(handler, chunks)` (an `hdlr` of that handler
/// type and an `stco`), every track's chunks inside the one `mdat` after `moov`: track `t`'s
/// chunk `i` is filled with byte `0x10 * t + i + 1`. An audiobook's chapter track is a second
/// `trak` with handler `text` whose samples also live in `mdat`.
pub fn m4a_with_tracks(brand: &[u8; 4], tracks: &[(&[u8; 4], u8)]) -> Vec<u8> {
    let moov = |offsets: &[Vec<u32>]| {
        let traks: Vec<Vec<u8>> = tracks
            .iter()
            .zip(offsets)
            .map(|((handler, _), offsets)| {
                let mut table = u32::try_from(offsets.len()).unwrap().to_be_bytes().to_vec();
                for offset in offsets {
                    table.extend(offset.to_be_bytes());
                }
                let hdlr_body = concat(&[&[0; 4], &handler[..], &[0; 12], b"\0"]);
                let stbl = atom(b"stbl", &atom(b"stco", &full(0, &table)));
                let mdia = concat(&[&atom(b"hdlr", &full(0, &hdlr_body)), &atom(b"minf", &stbl)]);
                atom(b"trak", &atom(b"mdia", &mdia))
            })
            .collect();
        let parts: Vec<&[u8]> = traks.iter().map(Vec::as_slice).collect();
        atom(b"moov", &concat(&[&mvhd_v0(0), &concat(&parts)]))
    };
    let head = ftyp(brand);
    let zeros: Vec<Vec<u32>> = tracks.iter().map(|(_, n)| vec![0; usize::from(*n)]).collect();
    let mut at = head.len() + moov(&zeros).len() + 8;
    let mut offsets = Vec::new();
    let mut payload = Vec::new();
    for (t, (_, chunks)) in tracks.iter().enumerate() {
        let mut table = Vec::new();
        for i in 0..*chunks {
            table.push(u32::try_from(at).unwrap());
            payload.extend([chunk_byte(t, usize::from(i)); CHUNK_LEN]);
            at += CHUNK_LEN;
        }
        offsets.push(table);
    }
    concat(&[&head, &moov(&offsets), &atom(b"mdat", &payload)])
}

/// The byte [`m4a_with_tracks`] fills track `track`'s chunk `chunk` with.
pub fn chunk_byte(track: usize, chunk: usize) -> u8 {
    u8::try_from(0x10 * track + chunk + 1).expect("a small test file")
}

/// The offsets in the first `stco` of an MP4 file's bytes.
pub fn stco_offsets(bytes: &[u8]) -> Vec<usize> {
    all_stco_offsets(bytes).into_iter().next().expect("an stco atom")
}

/// The offsets in every `stco` of an MP4 file's bytes, in file order.
pub fn all_stco_offsets(bytes: &[u8]) -> Vec<Vec<usize>> {
    let word = |pos: usize| u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
    bytes
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"stco")
        .map(|(at, _)| (0..word(at + 8)).map(|i| word(at + 12 + 4 * i)).collect())
        .collect()
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
