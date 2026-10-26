//! Read every managed field, the front cover, and the unmanaged text items from
//! an audio file. One private function per tag container.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{Picture, PictureInformation, PictureType};

use crate::format::{AudioFormat, format_of};
use crate::model::{Cover, FileTags, TagError, TagField};

/// The separator the editor joins multiple values with where a format stores one string.
const JOIN: &str = "; ";
/// The `mean` of every freeform MP4 atom the editor manages.
const ITUNES_MEAN: &str = "com.apple.iTunes";
/// What a picture without a MIME type is reported as.
const UNKNOWN_MIME: &str = "application/octet-stream";

fn parse_error(e: impl std::fmt::Display) -> TagError {
    TagError::Parse(e.to_string())
}

/// Reads the tags of the audio file at `path`. A file with no tag reads as empty tags.
/// Formats the editor doesn't handle (WAV, ...) are [`TagError::NotSupported`].
pub fn read_tags(path: &Path) -> Result<FileTags, TagError> {
    let format = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(format_of)
        .ok_or(TagError::NotSupported)?;
    let mut file = File::open(path)?;
    // Only tags are wanted; skipping the audio properties also spares a read of the stream.
    let options = ParseOptions::new().read_properties(false);
    match format {
        AudioFormat::Mp3 => {
            let file = MpegFile::read_from(&mut file, options).map_err(parse_error)?;
            Ok(file.id3v2().map(read_id3).unwrap_or_default())
        }
        AudioFormat::Mp4 => {
            let file = Mp4File::read_from(&mut file, options).map_err(parse_error)?;
            Ok(file.ilst().map(read_ilst).unwrap_or_default())
        }
        AudioFormat::Flac => {
            let file = FlacFile::read_from(&mut file, options).map_err(parse_error)?;
            let from_comments = file.vorbis_comments().map(|c| read_vorbis(c, file.pictures()));
            Ok(from_comments.unwrap_or_else(|| read_vorbis(&VorbisComments::new(), file.pictures())))
        }
        AudioFormat::OggVorbis => {
            let file = VorbisFile::read_from(&mut file, options).map_err(parse_error)?;
            let comments = file.vorbis_comments();
            Ok(read_vorbis(comments, comments.pictures()))
        }
        AudioFormat::Opus => {
            let file = OpusFile::read_from(&mut file, options).map_err(parse_error)?;
            let comments = file.vorbis_comments();
            Ok(read_vorbis(comments, comments.pictures()))
        }
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// `values` trimmed, with empties dropped.
fn clean<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    values
        .into_iter()
        .map(|v| v.as_ref().trim().to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Splits each value on [`JOIN`], then cleans.
fn split_joined(values: Vec<String>) -> Vec<String> {
    clean(values.iter().flat_map(|v| v.split(JOIN)))
}

/// Splits each value on ID3v2.4's null separator, then cleans.
fn split_nulls(value: &str) -> Vec<String> {
    clean(value.split('\0'))
}

/// A number field's value: only the part before a `/`.
fn number_part(values: Vec<String>) -> Vec<String> {
    clean(values.iter().map(|v| v.split('/').next().unwrap_or_default()))
}

/// Puts `values` under `field` unless they're empty. Multi-value fields keep every value;
/// number fields keep only the number.
fn put(tags: &mut FileTags, field: TagField, values: Vec<String>) {
    let values = match field {
        TagField::Track | TagField::Disc => number_part(values),
        _ => clean(values),
    };
    if !values.is_empty() {
        tags.fields.entry(field).or_default().extend(values);
    }
}

/// Adds an unmanaged item, merging repeated keys.
fn put_extra(tags: &mut FileTags, key: String, values: Vec<String>) {
    let values = clean(values);
    if !key.is_empty() && !values.is_empty() {
        tags.extras.entry(key).or_default().extend(values);
    }
}

/// The front cover among `pictures`: type `CoverFront`, else (when `fall_back`) the first.
fn pick_cover<'a>(pictures: impl Iterator<Item = &'a Picture>, fall_back: bool) -> Option<Cover> {
    let pictures: Vec<&Picture> = pictures.collect();
    let front = pictures.iter().find(|p| p.pic_type() == PictureType::CoverFront);
    let chosen = front.or_else(|| pictures.first().filter(|_| fall_back))?;
    Some(Cover {
        mime: chosen.mime_type().map_or(UNKNOWN_MIME, |m| m.as_str()).to_owned(),
        data: chosen.data().to_vec(),
    })
}

// ── ID3v2 ─────────────────────────────────────────────────────────────────────

/// What a managed TXXX description maps to; `None` for descriptions that are extras.
fn txxx_field(description: &str) -> Option<TagField> {
    const MAP: [(&str, TagField); 6] = [
        ("Performer", TagField::Performer),
        ("Translator", TagField::Translator),
        ("Description", TagField::Description),
        ("www", TagField::Www),
        ("Comment", TagField::Comment),
        ("DiscNumber", TagField::Disc),
    ];
    MAP.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(description))
        .map(|(_, field)| *field)
}

/// The field a text frame ID maps to.
fn frame_field(id: &str) -> Option<TagField> {
    Some(match id {
        "TIT2" => TagField::Title,
        "TPE1" => TagField::Artist,
        "TPE2" => TagField::AlbumArtist,
        "TALB" => TagField::Album,
        "TCOM" => TagField::Composer,
        "TDRC" | "TYER" => TagField::Date,
        "TCON" => TagField::Genre,
        "TPUB" => TagField::Publisher,
        "TRCK" => TagField::Track,
        "TPOS" => TagField::Disc,
        _ => return None,
    })
}

/// Puts an ID3 value under `field`: null-separated values, plus `"; "` for the multi fields.
fn put_id3(tags: &mut FileTags, field: TagField, raw: &str) {
    let values = split_nulls(raw);
    put(
        tags,
        field,
        if field.is_multi() { split_joined(values) } else { values },
    );
}

fn read_id3(id3: &Id3v2Tag) -> FileTags {
    let mut tags = FileTags::default();
    let mut legacy: BTreeMap<TagField, String> = BTreeMap::new();
    for frame in id3 {
        match frame {
            Frame::Text(f) => {
                if let Some(field) = frame_field(f.id().as_str()) {
                    put_id3(&mut tags, field, &f.value);
                }
            }
            Frame::Timestamp(f) => {
                if let Some(field) = frame_field(f.id().as_str()) {
                    put_id3(&mut tags, field, &f.timestamp.to_string());
                }
            }
            Frame::Comment(f) if f.description.is_empty() => put_id3(&mut tags, TagField::Comment, &f.content),
            Frame::UserText(f) => match txxx_field(&f.description) {
                // The legacy Comment and DiscNumber frames only fill a field COMM / TPOS left empty.
                Some(field @ (TagField::Comment | TagField::Disc)) => {
                    legacy.insert(field, f.content.to_string());
                }
                Some(field) => put_id3(&mut tags, field, &f.content),
                None => put_extra(&mut tags, f.description.to_string(), split_nulls(&f.content)),
            },
            _ => {}
        }
    }
    for (field, raw) in legacy {
        if !tags.fields.contains_key(&field) {
            put_id3(&mut tags, field, &raw);
        }
    }
    tags.cover = pick_cover(id3_pictures(id3), false);
    tags
}

/// Every attached picture of `id3`.
fn id3_pictures(id3: &Id3v2Tag) -> impl Iterator<Item = &Picture> {
    id3.into_iter().filter_map(|frame| match frame {
        Frame::Picture(f) => Some(f.picture.as_ref()),
        _ => None,
    })
}

// ── MP4 ilst ──────────────────────────────────────────────────────────────────

/// The text of every UTF-8 / UTF-16 data entry of an atom.
fn atom_texts<'a>(data: impl Iterator<Item = &'a AtomData>) -> Vec<String> {
    data.filter_map(|d| match d {
        AtomData::UTF8(s) | AtomData::UTF16(s) => Some(s.clone()),
        _ => None,
    })
    .collect()
}

/// The number in a `trkn` / `disk` atom: bytes 2-3 of its reserved-type data, 0 meaning none.
fn atom_number<'a>(mut data: impl Iterator<Item = &'a AtomData>) -> Vec<String> {
    match data.next() {
        Some(AtomData::Unknown { data, .. }) if data.len() >= 4 => {
            let number = u16::from_be_bytes([data[2], data[3]]);
            if number == 0 {
                Vec::new()
            } else {
                vec![number.to_string()]
            }
        }
        _ => Vec::new(),
    }
}

/// The field a four-character atom maps to.
fn fourcc_field(fourcc: &[u8; 4]) -> Option<TagField> {
    Some(match fourcc {
        b"\xA9alb" => TagField::Album,
        b"\xA9ART" => TagField::Artist,
        b"aART" => TagField::AlbumArtist,
        b"\xA9nam" => TagField::Title,
        b"\xA9day" => TagField::Date,
        b"\xA9cmt" => TagField::Comment,
        b"\xA9wrt" => TagField::Composer,
        b"trkn" => TagField::Track,
        b"disk" => TagField::Disc,
        _ => return None,
    })
}

/// The field a freeform atom name (under the iTunes mean) maps to.
fn freeform_field(name: &str) -> Option<TagField> {
    const MAP: [(&str, TagField); 6] = [
        ("PERFORMER", TagField::Performer),
        ("publisher", TagField::Publisher),
        ("www", TagField::Www),
        ("translator", TagField::Translator),
        ("description", TagField::Description),
        ("GENRE", TagField::Genre),
    ];
    MAP.iter()
        .find(|(managed, _)| managed.eq_ignore_ascii_case(name))
        .map(|(_, field)| *field)
}

fn read_ilst(ilst: &Ilst) -> FileTags {
    let mut tags = FileTags::default();
    let mut gen_values = Vec::new();
    for atom in ilst {
        match atom.ident() {
            AtomIdent::Fourcc([0xA9, b'g', b'e', b'n']) => gen_values = split_joined(atom_texts(atom.data())),
            AtomIdent::Fourcc(fourcc) => {
                let Some(field) = fourcc_field(fourcc) else { continue };
                let values = match field {
                    TagField::Track | TagField::Disc => atom_number(atom.data()),
                    _ if field.is_multi() => split_joined(atom_texts(atom.data())),
                    _ => atom_texts(atom.data()),
                };
                put(&mut tags, field, values);
            }
            AtomIdent::Freeform { mean, name } => {
                let field = (mean == ITUNES_MEAN).then(|| freeform_field(name)).flatten();
                let texts = atom_texts(atom.data());
                match field {
                    Some(field) if field.is_multi() => put(&mut tags, field, split_joined(texts)),
                    Some(field) => put(&mut tags, field, texts),
                    None => put_extra(&mut tags, name.to_string(), texts),
                }
            }
        }
    }
    // The freeform GENRE wins; `©gen` only fills in when it is absent, so values never double.
    if !tags.fields.contains_key(&TagField::Genre) {
        put(&mut tags, TagField::Genre, gen_values);
    }
    tags.cover = ilst.pictures().and_then(|p| pick_cover(p, true));
    tags
}

// ── Vorbis comments (FLAC, Ogg Vorbis, Opus) ──────────────────────────────────

/// The field a Vorbis key (uppercase) maps to.
fn vorbis_field(key: &str) -> Option<TagField> {
    Some(match key {
        "ALBUM" => TagField::Album,
        "ALBUMARTIST" => TagField::AlbumArtist,
        "ARTIST" => TagField::Artist,
        "TITLE" => TagField::Title,
        "TRACKNUMBER" => TagField::Track,
        "DATE" => TagField::Date,
        "GENRE" => TagField::Genre,
        "PUBLISHER" => TagField::Publisher,
        "PERFORMER" => TagField::Performer,
        "TRANSLATOR" => TagField::Translator,
        "DISCNUMBER" => TagField::Disc,
        "COMMENT" => TagField::Comment,
        "DESCRIPTION" => TagField::Description,
        "WWW" => TagField::Www,
        "COMPOSER" => TagField::Composer,
        _ => return None,
    })
}

fn read_vorbis(comments: &VorbisComments, pictures: &[(Picture, PictureInformation)]) -> FileTags {
    let mut tags = FileTags::default();
    for (key, value) in comments.items() {
        let key = key.to_ascii_uppercase();
        match vorbis_field(&key) {
            Some(field) => put(&mut tags, field, vec![value.to_owned()]),
            None => put_extra(&mut tags, key, vec![value.to_owned()]),
        }
    }
    tags.cover = pick_cover(pictures.iter().map(|(picture, _)| picture), true);
    tags
}
