//! Read every managed field, the front cover, and the unmanaged text items from
//! an audio file. One private function per tag container.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{Picture, PictureType};
use serde::{Deserialize, Serialize};

use crate::format::{AudioFormat, format_of};
use crate::model::{Cover, FileTags, TagError, TagField};

/// The separator the editor joins multiple values with where a format stores one string.
pub(crate) const JOIN: &str = "; ";
/// The `mean` of every freeform MP4 atom the editor manages.
pub(crate) const ITUNES_MEAN: &str = "com.apple.iTunes";
/// What a picture without a MIME type is reported as.
pub(crate) const UNKNOWN_MIME: &str = "application/octet-stream";

pub(crate) fn parse_error(e: impl std::fmt::Display) -> TagError {
    TagError::Parse(e.to_string())
}

/// Reads the tags of the audio file at `path`. A file with no tag reads as empty tags.
/// Formats the editor doesn't handle (WAV, ...) are [`TagError::NotSupported`].
pub fn read_tags(path: &Path) -> Result<FileTags, TagError> {
    let format = format_of_path(path)?;
    read_from(&mut File::open(path)?, format, parse_options())
}

/// Reads the tags of `format` from `source` with `options`.
pub(crate) fn read_from<R: Read + Seek>(
    source: &mut R,
    format: AudioFormat,
    options: ParseOptions,
) -> Result<FileTags, TagError> {
    read_both_from(source, format, options).map(|(tags, _)| tags)
}

/// What a file holds beyond [`FileTags`] that a write can change and an undo must put back:
/// the track and disc totals (the reader shows plain numbers) and, on MP3, the conductor
/// (`TPE3`, which an audiobook write removes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawTags {
    pub(crate) track_total: Option<u32>,
    pub(crate) disc_total: Option<u32>,
    pub(crate) conductor: Option<String>,
}

/// Reads the [`RawTags`] of the audio file at `path`, parsing as [`read_tags`] does.
#[cfg(test)]
pub(crate) fn read_raw(path: &Path) -> Result<RawTags, TagError> {
    read_snapshot(path).map(|(_, raw)| raw)
}

/// The [`FileTags`] and [`RawTags`] of the audio file at `path` from one parse, so both
/// describe the same state of the file (the undo store's snapshot).
pub(crate) fn read_snapshot(path: &Path) -> Result<(FileTags, RawTags), TagError> {
    let format = format_of_path(path)?;
    read_both_from(&mut File::open(path)?, format, parse_options())
}

/// The [`FileTags`] and [`RawTags`] of `format` from `source` with `options`, from one parse.
/// The writer reads the file it just saved through here too, with its own strict options.
pub(crate) fn read_both_from<R: Read + Seek>(
    source: &mut R,
    format: AudioFormat,
    options: ParseOptions,
) -> Result<(FileTags, RawTags), TagError> {
    let empty = VorbisComments::new();
    Ok(match format {
        AudioFormat::Mp3 => {
            let file = MpegFile::read_from(source, options).map_err(parse_error)?;
            file.id3v2().map(|t| (read_id3(t), raw_id3(t))).unwrap_or_default()
        }
        AudioFormat::Mp4 => {
            let file = Mp4File::read_from(source, options).map_err(parse_error)?;
            file.ilst().map(|t| (read_ilst(t), raw_ilst(t))).unwrap_or_default()
        }
        AudioFormat::Flac => {
            let file = FlacFile::read_from(source, options).map_err(parse_error)?;
            let comments = file.vorbis_comments().unwrap_or(&empty);
            (read_vorbis(comments, flac_pictures(&file)), raw_vorbis(comments))
        }
        AudioFormat::OggVorbis => {
            let file = VorbisFile::read_from(source, options).map_err(parse_error)?;
            let comments = file.vorbis_comments();
            (
                read_vorbis(comments, comments.pictures().iter().map(|(p, _)| p)),
                raw_vorbis(comments),
            )
        }
        AudioFormat::Opus => {
            let file = OpusFile::read_from(source, options).map_err(parse_error)?;
            let comments = file.vorbis_comments();
            (
                read_vorbis(comments, comments.pictures().iter().map(|(p, _)| p)),
                raw_vorbis(comments),
            )
        }
    })
}

/// The total in `"N/T"`; none for a plain number, a zero, or text that isn't a number.
fn total_after_slash(value: &str) -> Option<u32> {
    let (_, total) = value.split_once('/')?;
    total.trim().parse().ok().filter(|t| *t > 0)
}

pub(crate) fn raw_id3(id3: &Id3v2Tag) -> RawTags {
    let text = |id: &'static str| id3.get_text(&FrameId::Valid(id.into()));
    RawTags {
        track_total: text("TRCK").and_then(total_after_slash),
        disc_total: text("TPOS").and_then(total_after_slash),
        conductor: text("TPE3").map(str::trim).filter(|c| !c.is_empty()).map(str::to_owned),
    }
}

fn raw_ilst(ilst: &Ilst) -> RawTags {
    // Bytes 4-5 of a `trkn` / `disk` atom's data, 0 meaning none.
    let total = |fourcc: [u8; 4]| match ilst.get(&AtomIdent::Fourcc(fourcc)).and_then(|a| a.data().next()) {
        Some(AtomData::Unknown { data, .. }) if data.len() >= 6 => {
            Some(u32::from(u16::from_be_bytes([data[4], data[5]]))).filter(|t| *t > 0)
        }
        _ => None,
    };
    RawTags {
        track_total: total(*b"trkn"),
        disc_total: total(*b"disk"),
        conductor: None,
    }
}

fn raw_vorbis(comments: &VorbisComments) -> RawTags {
    // A total key wins over a `N/T` number.
    let total = |keys: [&str; 2], number: &str| {
        let keyed = comments.items().find_map(|(key, value)| {
            keys.iter()
                .any(|k| k.eq_ignore_ascii_case(key))
                .then(|| value.trim().parse().ok().filter(|t: &u32| *t > 0))
                .flatten()
        });
        keyed.or_else(|| {
            comments
                .items()
                .find(|(key, _)| key.eq_ignore_ascii_case(number))
                .and_then(|(_, value)| total_after_slash(value))
        })
    };
    RawTags {
        track_total: total(["TRACKTOTAL", "TOTALTRACKS"], "TRACKNUMBER"),
        disc_total: total(["DISCTOTAL", "TOTALDISCS"], "DISCNUMBER"),
        conductor: None,
    }
}

/// A FLAC's pictures in the order the cover is picked from: the picture blocks, then the
/// pictures in the comments (`METADATA_BLOCK_PICTURE`). The writer merges them in this order.
fn flac_pictures(file: &FlacFile) -> impl Iterator<Item = &Picture> + Clone {
    let in_comments = file.vorbis_comments().map(|c| c.pictures()).unwrap_or_default();
    file.pictures().iter().chain(in_comments).map(|(picture, _)| picture)
}

/// Only tags are wanted; skipping the audio properties also spares a read of the stream.
/// Saving a tag never needs them: lofty's tag writers re-read the container themselves.
pub(crate) fn parse_options() -> ParseOptions {
    ParseOptions::new().read_properties(false)
}

/// The format of `path`, by its file name.
pub(crate) fn format_of_path(path: &Path) -> Result<AudioFormat, TagError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(format_of)
        .ok_or(TagError::NotSupported)
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
/// every other field keeps only the first value found. Number fields keep only the number.
fn put(tags: &mut FileTags, field: TagField, values: Vec<String>) {
    let mut values = match field {
        TagField::Track | TagField::Disc => number_part(values),
        _ => clean(values),
    };
    if !field.is_multi() {
        if tags.fields.contains_key(&field) {
            return;
        }
        values.truncate(1);
    }
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

/// Which of `pictures` is the cover: the first of type `CoverFront`, else the first untyped
/// one (`Other`, which is also every MP4 `covr` picture). A typed picture such as a back
/// cover or a booklet page is never the cover, so removing the cover leaves it alone and
/// setting one beside it adds a front cover. The writer replaces or removes exactly this one.
pub(crate) fn cover_position<'a>(pictures: impl Iterator<Item = &'a Picture>) -> Option<usize> {
    let types: Vec<PictureType> = pictures.map(Picture::pic_type).collect();
    let first = |kind: PictureType| types.iter().position(|t| *t == kind);
    first(PictureType::CoverFront).or_else(|| first(PictureType::Other))
}

/// The cover among `pictures` (see [`cover_position`]).
fn pick_cover<'a>(pictures: impl Iterator<Item = &'a Picture> + Clone) -> Option<Cover> {
    let chosen = pictures.clone().nth(cover_position(pictures)?)?;
    Some(Cover {
        mime: chosen.mime_type().map_or(UNKNOWN_MIME, |m| m.as_str()).to_owned(),
        data: chosen.data().to_vec(),
    })
}

// ── ID3v2 ─────────────────────────────────────────────────────────────────────

/// What a managed TXXX description maps to; `None` for descriptions that are extras.
pub(crate) fn txxx_field(description: &str) -> Option<TagField> {
    TXXX_FIELDS
        .iter()
        .chain(&TXXX_LEGACY_FIELDS)
        .find(|(name, _)| name.eq_ignore_ascii_case(description))
        .map(|(_, field)| *field)
}

/// The TXXX descriptions the writer writes.
pub(crate) const TXXX_FIELDS: [(&str, TagField); 4] = [
    ("Performer", TagField::Performer),
    ("Translator", TagField::Translator),
    ("Description", TagField::Description),
    ("www", TagField::Www),
];

/// TXXX descriptions only read (COMM and TPOS are written instead) and removed on write.
const TXXX_LEGACY_FIELDS: [(&str, TagField); 2] = [("Comment", TagField::Comment), ("DiscNumber", TagField::Disc)];

/// The field a text frame ID maps to.
pub(crate) fn frame_field(id: &str) -> Option<TagField> {
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

pub(crate) fn read_id3(id3: &Id3v2Tag) -> FileTags {
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
    tags.cover = pick_cover(id3_pictures(id3));
    tags
}

/// Every attached picture of `id3`.
pub(crate) fn id3_pictures(id3: &Id3v2Tag) -> impl Iterator<Item = &Picture> + Clone {
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
pub(crate) fn fourcc_field(fourcc: &[u8; 4]) -> Option<TagField> {
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

/// The managed freeform atom names (under the iTunes mean), as the writer writes them.
pub(crate) const FREEFORM_FIELDS: [(&str, TagField); 6] = [
    ("PERFORMER", TagField::Performer),
    ("publisher", TagField::Publisher),
    ("www", TagField::Www),
    ("translator", TagField::Translator),
    ("description", TagField::Description),
    ("GENRE", TagField::Genre),
];

/// The field a freeform atom name (under the iTunes mean) maps to.
pub(crate) fn freeform_field(name: &str) -> Option<TagField> {
    FREEFORM_FIELDS
        .iter()
        .find(|(managed, _)| managed.eq_ignore_ascii_case(name))
        .map(|(_, field)| *field)
}

pub(crate) fn read_ilst(ilst: &Ilst) -> FileTags {
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
    tags.cover = ilst
        .pictures()
        .and_then(|p| pick_cover(p.collect::<Vec<_>>().into_iter()));
    tags
}

// ── Vorbis comments (FLAC, Ogg Vorbis, Opus) ──────────────────────────────────

/// Every managed Vorbis key, as the writer writes it.
pub(crate) const VORBIS_FIELDS: [(&str, TagField); 15] = [
    ("ALBUM", TagField::Album),
    ("ALBUMARTIST", TagField::AlbumArtist),
    ("ARTIST", TagField::Artist),
    ("TITLE", TagField::Title),
    ("TRACKNUMBER", TagField::Track),
    ("DATE", TagField::Date),
    ("GENRE", TagField::Genre),
    ("PUBLISHER", TagField::Publisher),
    ("PERFORMER", TagField::Performer),
    ("TRANSLATOR", TagField::Translator),
    ("DISCNUMBER", TagField::Disc),
    ("COMMENT", TagField::Comment),
    ("DESCRIPTION", TagField::Description),
    ("WWW", TagField::Www),
    ("COMPOSER", TagField::Composer),
];

/// Track and disc totals, which the writer manages too: it writes `TRACKTOTAL` when the
/// caller passes a total and removes every other one. Never an extra.
pub(crate) const VORBIS_TOTALS: [&str; 4] = ["TRACKTOTAL", "TOTALTRACKS", "DISCTOTAL", "TOTALDISCS"];

/// The field a Vorbis key (uppercase) maps to.
pub(crate) fn vorbis_field(key: &str) -> Option<TagField> {
    VORBIS_FIELDS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, field)| *field)
}

pub(crate) fn read_vorbis<'a>(
    comments: &VorbisComments,
    pictures: impl Iterator<Item = &'a Picture> + Clone,
) -> FileTags {
    let mut tags = FileTags::default();
    for (key, value) in comments.items() {
        let key = key.to_ascii_uppercase();
        match vorbis_field(&key) {
            Some(field) => put(&mut tags, field, vec![value.to_owned()]),
            None if VORBIS_TOTALS.contains(&key.as_str()) => {}
            None => put_extra(&mut tags, key, vec![value.to_owned()]),
        }
    }
    tags.cover = pick_cover(pictures);
    tags
}
