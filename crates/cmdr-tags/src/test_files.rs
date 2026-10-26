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
use lofty::picture::PictureInformation;
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
    tag.insert(Frame::Comment(CommentFrame::new(
        TextEncoding::UTF8,
        *b"eng",
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
/// `cover_len` bytes when it is non-zero.
pub fn write_flac(dir: &Path, name: &str, mut comments: VorbisComments, cover_len: usize) -> PathBuf {
    if cover_len > 0 {
        comments
            .insert_picture(cover(cover_len), Some(PictureInformation::default()))
            .expect("a picture lofty accepts");
    }
    let mut cursor = Cursor::new(flac_stream());
    comments
        .save_to(&mut cursor, WriteOptions::default())
        .expect("lofty writes the Vorbis comments");
    write_bytes(dir, name, cursor.into_inner())
}

/// A file of arbitrary bytes.
pub fn write_bytes(dir: &Path, name: &str, bytes: Vec<u8>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("the temp dir is writable");
    path
}
