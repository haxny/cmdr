//! Write the managed fields and the cover into an audio file, remove the extras the caller
//! dropped and add the ones the file lacks, leaving every other frame, atom, and key as it was. One private function
//! per tag container, all saved through [`save_safely`].
//!
//! Each writer is the inverse of its reader in `read.rs`: it removes everything the reader
//! maps to a managed field (aliases and duplicates included), then writes one frame, atom,
//! or key per field.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Seek;
use std::path::Path;

use lofty::TextEncoding;
use lofty::config::{ParseOptions, ParsingMode, WriteOptions as LoftyWriteOptions};
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{
    AttachedPictureFrame, CommentFrame, ExtendedTextFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame,
    TimestampFrame,
};
use lofty::mp4::{Atom, AtomData, AtomIdent, DataType, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::tag::TagExt;
use lofty::tag::items::Timestamp;
use serde::{Deserialize, Serialize};

use crate::format::AudioFormat;
use crate::model::{ContentKind, Cover, FileTags, TagError, TagField};
use crate::read::{
    FREEFORM_FIELDS, ITUNES_MEAN, JOIN, RawTags, TXXX_FIELDS, UNKNOWN_MIME, VORBIS_FIELDS, VORBIS_TOTALS,
    cover_position, format_of_path, fourcc_field, frame_field, freeform_field, hidden_items, id3_items, id3_pictures,
    ilst_items, parse_error, parse_options, raw_id3, read_both_from, read_id3, read_ilst, read_vorbis, txxx_field,
    vorbis_field, vorbis_items,
};
use crate::save::save_safely;
use crate::write_extras::{added_extras, dropped_extras, upper_extra_keys};

/// How a write maps fields.
///
/// The default is lossless music: [`ContentKind::Music`] (an audiobook write removes ID3's
/// `TPE3`, which the caller must choose deliberately), no totals, and no frame loss allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOptions {
    /// Audiobooks drop ID3's `TPE3` (conductor), which `FileTags` doesn't hold. The composer
    /// stays: it holds the author of the book's music.
    pub kind: ContentKind,
    /// The number of tracks, when the caller knows it: the track is then written `N/Total`
    /// (MP3), with `trkn`'s total (M4A), or with a `TRACKTOTAL` key (Vorbis); otherwise plain.
    pub track_total: Option<u32>,
    /// The number of discs, when the caller knows it: the disc is then written `N/Total`
    /// (MP3), with `disk`'s total (M4A), or with a `DISCTOTAL` key (Vorbis); otherwise plain.
    pub disc_total: Option<u32>,
    /// Whether an MP3 write may drop the ID3v2.4-only frames ID3v2.3 has no place for
    /// (`TSOP`, `RVA2`, ...). When false, such a write fails with [`TagError::WouldDrop`],
    /// naming them, and the file stays as it was.
    pub allow_frame_loss: bool,
}

/// Writes `tags` into the audio file at `path` with a safe overwrite. Afterwards the file
/// holds exactly `tags.fields` for the managed fields (an absent field is removed),
/// `tags.cover` as the cover (`None` removes it; other pictures stay), and the extras
/// `tags.extras` has a key for: one the file has is left as the file has it, one it lacks is
/// added (ID3 `TXXX`, MP4 freeform under the iTunes mean, a Vorbis key; so an undo can put
/// back an extra an edit removed), and one absent from `tags.extras` is removed. Everything
/// else in the file is untouched.
///
/// An extra the file lacks that can't be added (named like a managed key, say) fails the
/// write as [`TagError::UnwritableExtra`].
///
/// The file is parsed strictly first (an item lofty can't decode refuses the write as
/// [`TagError::Parse`] rather than vanishing from the saved file), and the saved copy must
/// read back as exactly the intended tags before it replaces the original.
pub fn write_tags(path: &Path, tags: &FileTags, opts: &WriteOptions) -> Result<(), TagError> {
    write_with(path, tags, opts, None)
}

/// Writes a snapshot back as it was: `tags`, the totals in `raw`, and on MP3 the conductor
/// (`TPE3`) when `raw` has one, as music so a conductor stays. The undo store's
/// write.
pub(crate) fn restore_tags(
    path: &Path,
    tags: &FileTags,
    raw: &RawTags,
    allow_frame_loss: bool,
) -> Result<(), TagError> {
    let opts = WriteOptions {
        kind: ContentKind::Music,
        track_total: raw.track_total,
        disc_total: raw.disc_total,
        allow_frame_loss,
    };
    write_with(path, tags, &opts, raw.conductor.as_deref())
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            kind: ContentKind::Music,
            track_total: None,
            disc_total: None,
            allow_frame_loss: false,
        }
    }
}

/// [`write_tags`], plus a `TPE3` to write on MP3 (`None` leaves the file's, or for an
/// audiobook removes it).
fn write_with(path: &Path, tags: &FileTags, opts: &WriteOptions, conductor: Option<&str>) -> Result<(), TagError> {
    let format = format_of_path(path)?;
    // A zero total is no total: MP4 stores none as zero.
    let opts = &WriteOptions {
        track_total: opts.track_total.filter(|t| *t > 0),
        disc_total: opts.disc_total.filter(|t| *t > 0),
        ..*opts
    };
    let tags = &match format {
        AudioFormat::Flac | AudioFormat::OggVorbis | AudioFormat::Opus => upper_extra_keys(tags),
        AudioFormat::Mp3 | AudioFormat::Mp4 => tags.clone(),
    };
    let values = values_to_write(tags);
    let mut source = File::open(path)?;
    let parse = write_parse_options(format);
    let defaults = LoftyWriteOptions::default();
    match format {
        AudioFormat::Mp3 => {
            let file = MpegFile::read_from(&mut source, parse).map_err(parse_error)?;
            if file.id3v2().is_none() {
                refuse_tag_behind_junk(&mut source)?;
            }
            let mut id3 = file.id3v2().cloned().unwrap_or_default();
            let current = read_id3(&id3);
            let hidden = hidden_items(&id3_items(&id3), &current);
            let conductor = match (conductor, opts.kind) {
                (Some(conductor), _) => Some(conductor.to_owned()),
                (None, ContentKind::Audiobook) => None,
                (None, ContentKind::Music) => raw_id3(&id3).conductor,
            };
            write_id3(&mut id3, &current, tags, &values, opts, conductor.as_deref())?;
            refuse_loss(hidden.into_iter().chain(v23_losses(&id3)), opts.allow_frame_loss)?;
            let expected = expected_tags(&current, tags, &values, format, &read_id3(&id3))?;
            let raw = expected_raw(&values, opts, conductor);
            let v23 = LoftyWriteOptions::new().use_id3v23(true);
            save_safely(path, |out| save(&id3, out, v23, format, &expected, &raw))
        }
        AudioFormat::Mp4 => {
            let file = Mp4File::read_from(&mut source, parse).map_err(parse_error)?;
            let mut ilst = file.ilst().cloned().unwrap_or_default();
            let current = read_ilst(&ilst);
            refuse_loss(hidden_items(&ilst_items(&ilst), &current), opts.allow_frame_loss)?;
            write_ilst(&mut ilst, &current, tags, &values, opts)?;
            let expected = expected_tags(&current, tags, &values, format, &read_ilst(&ilst))?;
            let raw = expected_raw(&values, opts, None);
            save_safely(path, |out| save(&ilst, out, defaults, format, &expected, &raw))
        }
        AudioFormat::Flac => {
            let mut file = FlacFile::read_from(&mut source, parse).map_err(parse_error)?;
            let mut comments = file.vorbis_comments().cloned().unwrap_or_default();
            // Saving the comments rewrites every FLAC picture block from the comments' own
            // pictures, so all of them move there first, in the reader's order (blocks, then
            // the comments' own): the cover is then picked from the same list.
            let in_comments = comments.remove_pictures();
            for (picture, info) in file.remove_pictures().into_iter().chain(in_comments) {
                // lofty keeps one icon per icon type and hands back the one it replaced.
                if let Some((dropped, _)) = comments.insert_picture(picture, Some(info)).map_err(write_error)? {
                    return Err(TagError::WouldDrop(vec![format!("PICTURE:{:?}", dropped.pic_type())]));
                }
            }
            let (comments, expected) = write_vorbis_family(&comments, tags, &values, opts, format)?;
            let raw = expected_raw(&values, opts, None);
            save_safely(path, |out| save(&comments, out, defaults, format, &expected, &raw))
        }
        AudioFormat::OggVorbis => {
            let file = VorbisFile::read_from(&mut source, parse).map_err(parse_error)?;
            let (comments, expected) = write_vorbis_family(file.vorbis_comments(), tags, &values, opts, format)?;
            let raw = expected_raw(&values, opts, None);
            save_safely(path, |out| save(&comments, out, defaults, format, &expected, &raw))
        }
        AudioFormat::Opus => {
            let file = OpusFile::read_from(&mut source, parse).map_err(parse_error)?;
            let (comments, expected) = write_vorbis_family(file.vorbis_comments(), tags, &values, opts, format)?;
            let raw = expected_raw(&values, opts, None);
            save_safely(path, |out| save(&comments, out, defaults, format, &expected, &raw))
        }
    }
}

/// How a write parses: strictly, so an item lofty can't decode fails the write instead of
/// being dropped from the copy that gets saved. Vorbis comments also without implicit
/// conversions, so lofty keeps `TRACKNUMBER` as the file has it instead of splitting a
/// `3/12` into a `TRACKTOTAL` key of its own.
fn write_parse_options(format: AudioFormat) -> ParseOptions {
    let mut strict = parse_options().parsing_mode(ParsingMode::Strict);
    match format {
        AudioFormat::Flac | AudioFormat::OggVorbis | AudioFormat::Opus => strict.implicit_conversions(false),
        AudioFormat::Mp3 | AudioFormat::Mp4 => strict,
    }
}

/// A strict parse doesn't search for an ID3v2 tag behind junk bytes before the audio; the
/// lenient one does. Writing after only the strict one finds nothing would put a second tag
/// in front of the one the reader shows, so such a file is refused.
fn refuse_tag_behind_junk(source: &mut File) -> Result<(), TagError> {
    source.rewind()?;
    let lenient = MpegFile::read_from(source, parse_options()).map_err(parse_error)?;
    if lenient.id3v2().is_some() {
        return Err(TagError::Parse("the ID3v2 tag doesn't start the file".to_owned()));
    }
    Ok(())
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// The managed values a write puts in the file, by field: trimmed, empties dropped, one
/// value for a single-value field. A field missing here is removed from the file.
type Values = BTreeMap<TagField, Vec<String>>;

fn values_to_write(tags: &FileTags) -> Values {
    tags.fields
        .iter()
        .filter_map(|(field, values)| {
            let limit = if field.is_multi() { usize::MAX } else { 1 };
            let values: Vec<String> = values
                .iter()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .take(limit)
                .map(str::to_owned)
                .collect();
            (!values.is_empty()).then_some((*field, values))
        })
        .collect()
}

/// What the saved file must read back as: `values` as the reader of `format` reports them
/// (one-string formats split joined values; numbers and ID3 dates come back normalized),
/// `tags.cover`, and the file's extras that `tags` kept. With the cover removed, the reader
/// falls back to another picture: `edited` is the tag as it will be saved, which says which.
fn expected_tags(
    current: &FileTags,
    tags: &FileTags,
    values: &Values,
    format: AudioFormat,
    edited: &FileTags,
) -> Result<FileTags, TagError> {
    let mut fields = BTreeMap::new();
    for (field, values) in values {
        let read_back: Vec<String> = match field {
            TagField::Track | TagField::Disc => values.iter().map(|v| number_text(v, None)).collect(),
            TagField::Date if format == AudioFormat::Mp3 => vec![timestamp(&values[0])?.to_string()],
            // The readers split joined values of a multi-value field in every format.
            _ if field.is_multi() => values
                .iter()
                .flat_map(|v| v.split(JOIN))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => values.clone(),
        };
        let read_back = match (field, format) {
            // `trkn` / `disk` hold a number, so "03" reads back as "3".
            (TagField::Track | TagField::Disc, AudioFormat::Mp4) => read_back
                .iter()
                .map(|v| as_u16(v).map(|n| n.to_string()))
                .collect::<Result<_, _>>()?,
            _ => read_back,
        };
        if !read_back.is_empty() {
            fields.insert(*field, read_back);
        }
    }
    let extras = tags
        .extras
        .keys()
        .filter_map(|key| current.extras.get_key_value(key))
        .map(|(key, values)| (key.clone(), values.clone()))
        .chain(added_extras(current, tags, format)?)
        .collect();
    Ok(FileTags {
        fields,
        cover: tags.cover.clone().or_else(|| edited.cover.clone()),
        extras,
    })
}

/// What the saved file's [`RawTags`] must be: each total when its number is written, and
/// `conductor`.
fn expected_raw(values: &Values, opts: &WriteOptions, conductor: Option<String>) -> RawTags {
    RawTags {
        track_total: opts.track_total.filter(|_| values.contains_key(&TagField::Track)),
        disc_total: opts.disc_total.filter(|_| values.contains_key(&TagField::Disc)),
        conductor,
    }
}

/// A track or disc number as text: the number before any `/`, plus `/total` when known.
fn number_text(value: &str, total: Option<u32>) -> String {
    let number = value.split('/').next().unwrap_or_default().trim();
    match total {
        Some(total) => format!("{number}/{total}"),
        None => number.to_owned(),
    }
}

/// A track or disc number (or total) as MP4 stores it.
fn as_u16(text: &str) -> Result<u16, TagError> {
    text.trim()
        .parse::<u16>()
        .map_err(|e| TagError::Write(format!("{text:?} isn't a track or disc number: {e}")))
}

fn write_error(e: impl std::fmt::Display) -> TagError {
    TagError::Write(e.to_string())
}

/// `cover` as a lofty picture of type `kind`. The reader reports a picture without a MIME
/// type as [`UNKNOWN_MIME`]; that goes back to no MIME type.
fn picture(cover: &Cover, kind: PictureType) -> Picture {
    let builder = Picture::unchecked(cover.data.clone()).pic_type(kind);
    if cover.mime == UNKNOWN_MIME {
        builder.build()
    } else {
        builder.mime_type(MimeType::from_str(&cover.mime)).build()
    }
}

/// Saves `tag` into `out`, then reads `out` back as the writer parses and checks it holds
/// `expected` and `raw`. lofty has corrupted a container on save (0.25.4 truncates a FLAC whose
/// metadata runs to the end of the file when the metadata shrinks) and drops what it can't
/// encode; a copy that doesn't parse or reads back differently never replaces the original.
fn save<T: TagExt>(
    tag: &T,
    out: &mut File,
    options: LoftyWriteOptions,
    format: AudioFormat,
    expected: &FileTags,
    raw: &RawTags,
) -> Result<(), TagError> {
    tag.save_to(out, options).map_err(write_error)?;
    let unparseable = |e: TagError| TagError::Write(format!("the written file doesn't parse: {e}"));
    out.rewind()?;
    let (written, written_raw) = read_both_from(out, format, write_parse_options(format)).map_err(unparseable)?;
    let differs: Vec<&str> = [
        ("fields", written.fields != expected.fields),
        ("cover", written.cover != expected.cover),
        ("extras", written.extras != expected.extras),
        ("totals or conductor", written_raw != *raw),
    ]
    .into_iter()
    .filter_map(|(part, differs)| differs.then_some(part))
    .collect();
    if differs.is_empty() {
        Ok(())
    } else {
        Err(TagError::Write(format!(
            "the written file reads back with other {}",
            differs.join(", ")
        )))
    }
}

// ── ID3v2 ─────────────────────────────────────────────────────────────────────

/// Whether the writer replaces `frame`: everything the reader maps to a field (TYER and
/// TDRC, COMM in any language, the legacy TXXX:Comment and TXXX:DiscNumber), ID3v2.3's
/// TDAT and TIME (parts of the date), and for audiobooks TPE3. Pictures are handled apart.
fn is_managed_id3(frame: &Frame<'_>, kind: ContentKind) -> bool {
    match frame {
        Frame::Comment(f) => f.description.is_empty(),
        Frame::UserText(f) => txxx_field(&f.description).is_some(),
        Frame::Picture(_) => false,
        _ => {
            let id = frame.id_str();
            frame_field(id).is_some()
                || matches!(id, "TDAT" | "TIME")
                || (kind == ContentKind::Audiobook && id == "TPE3")
        }
    }
}

/// The ID3v2.4 frames lofty 0.25.4's ID3v2.3 writer discards (`FRAMES_TO_DISCARD` in
/// `id3/v2/write/frame.rs`): ID3v2.3 has no place for them.
const DROPPED_BY_V23: [&str; 14] = [
    "ASPI", "EQU2", "RVA2", "SEEK", "SIGN", "TDEN", "TDRL", "TDTG", "TMOO", "TPRO", "TSOA", "TSOP", "TSOT", "TSST",
];

fn write_id3(
    id3: &mut Id3v2Tag,
    current: &FileTags,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
    conductor: Option<&str>,
) -> Result<(), TagError> {
    let frames = values
        .iter()
        .map(|(field, values)| id3_frame(*field, values, opts))
        .collect::<Result<Vec<_>, _>>()?;
    let dropped = dropped_extras(current, tags);
    let cover_at = cover_position(id3_pictures(id3));
    let cover_kept = tags.cover == current.cover;
    let mut new_cover = tags.cover.as_ref().filter(|_| !cover_kept).map(|cover| {
        Frame::Picture(AttachedPictureFrame::new(
            TextEncoding::UTF8,
            picture(cover, PictureType::CoverFront),
        ))
    });
    let mut picture_index = 0;
    // The cover is replaced where it stands, never re-inserted: `insert` would also replace any
    // other picture lofty considers equal to the new one.
    id3.retain_mut(|frame| match frame {
        Frame::Picture(_) => {
            picture_index += 1;
            if cover_kept || Some(picture_index - 1) != cover_at {
                return true;
            }
            match new_cover.take() {
                Some(new) => {
                    *frame = new;
                    true
                }
                None => false,
            }
        }
        Frame::UserText(f) if dropped.contains(f.description.as_ref()) => false,
        _ => !is_managed_id3(frame, opts.kind),
    });
    // An extra's values go in one TXXX, null-separated, which the reader splits again.
    let extras = added_extras(current, tags, AudioFormat::Mp3)?
        .into_iter()
        .map(|(key, values)| Frame::UserText(ExtendedTextFrame::new(TextEncoding::UTF8, key, values.join("\0"))));
    let conductor = conductor.map(|c| text_frame("TPE3", c.to_owned()));
    for frame in frames.into_iter().chain(extras).chain(conductor).chain(new_cover) {
        id3.insert(frame);
    }
    Ok(())
}

/// The frames lofty's ID3v2.3 writer would lose from `id3`: v2.4-only and outdated v2.2 IDs
/// (dropped), a `TDOR` finer than a year (cut to the year as `TORY`), and `TMCL` (merged with
/// `TIPL` into one `IPLS`, which reads back as `TIPL`; a lone `TIPL` round-trips).
fn v23_losses(id3: &Id3v2Tag) -> Vec<String> {
    id3.into_iter()
        .filter(|f| match f {
            Frame::Timestamp(t) if f.id_str() == "TDOR" => t.timestamp.month.is_some(),
            _ => !f.id().is_valid() || DROPPED_BY_V23.contains(&f.id_str()) || f.id_str() == "TMCL",
        })
        .map(|f| f.id_str().to_owned())
        .collect()
}

/// [`TagError::WouldDrop`] naming what the write would lose (sorted, each once), unless
/// nothing or the caller allows it. Two kinds of loss: items the reader hides
/// ([`hidden_items`]: a second value of a single-value field, a differing legacy `TXXX`, a
/// `COMM` in another language, ...; one equal to what the reader shows goes silently), and on
/// MP3 the frames ID3v2.3 has no place for ([`v23_losses`]).
fn refuse_loss(lost: impl IntoIterator<Item = String>, allowed: bool) -> Result<(), TagError> {
    let lost: BTreeSet<String> = lost.into_iter().collect();
    if lost.is_empty() || allowed {
        Ok(())
    } else {
        Err(TagError::WouldDrop(lost.into_iter().collect()))
    }
}

/// The one frame `field` is written as. Text is UTF-8 here; lofty re-encodes it as UTF-16
/// for ID3v2.3.
fn id3_frame(field: TagField, values: &[String], opts: &WriteOptions) -> Result<Frame<'static>, TagError> {
    let joined = values.join(JOIN);
    let text = text_frame;
    Ok(match field {
        TagField::Title => text("TIT2", joined),
        TagField::Artist => text("TPE1", joined),
        TagField::AlbumArtist => text("TPE2", joined),
        TagField::Album => text("TALB", joined),
        TagField::Composer => text("TCOM", joined),
        TagField::Genre => text("TCON", joined),
        TagField::Publisher => text("TPUB", joined),
        TagField::Track => text("TRCK", number_text(&joined, opts.track_total)),
        TagField::Disc => text("TPOS", number_text(&joined, opts.disc_total)),
        // A timestamp frame, which lofty splits into TYER (+ TDAT, TIME) for ID3v2.3.
        TagField::Date => Frame::Timestamp(TimestampFrame::new(
            FrameId::Valid(Cow::Borrowed("TDRC")),
            TextEncoding::UTF8,
            timestamp(&joined)?,
        )),
        TagField::Comment => Frame::Comment(CommentFrame::new(TextEncoding::UTF8, *b"eng", String::new(), joined)),
        TagField::Performer | TagField::Translator | TagField::Description | TagField::Www => {
            let description = TXXX_FIELDS
                .iter()
                .find(|(_, f)| *f == field)
                .map_or_else(|| unreachable_field(field), |(name, _)| Ok(*name))?;
            Frame::UserText(ExtendedTextFrame::new(
                TextEncoding::UTF8,
                description.to_owned(),
                joined,
            ))
        }
    })
}

/// A text information frame such as `TIT2`, in UTF-8 (lofty re-encodes it for ID3v2.3).
fn text_frame(id: &'static str, value: String) -> Frame<'static> {
    Frame::Text(TextInformationFrame::new(
        FrameId::Valid(Cow::Borrowed(id)),
        TextEncoding::UTF8,
        value,
    ))
}

/// A date lofty can write. lofty drops a date it can't parse from an ID3v2.3 tag without a
/// word, so an unparseable one fails the write instead.
fn timestamp(value: &str) -> Result<Timestamp, TagError> {
    let parsed: Timestamp = value
        .parse()
        .map_err(|e| TagError::Write(format!("date {value:?} isn't a timestamp: {e}")))?;
    // The range check lofty's ID3v2.3 writer applies (its own `verify` is private).
    let in_range = |field: Option<u8>, limit: u8| field.is_none_or(|f| f <= limit);
    let valid = parsed.year <= 9999
        && in_range(parsed.month, 12)
        && in_range(parsed.day, 31)
        && in_range(parsed.hour, 23)
        && in_range(parsed.minute, 59)
        && in_range(parsed.second, 59);
    if !valid {
        return Err(TagError::Write(format!("date {value:?} is out of range")));
    }
    Ok(parsed)
}

fn unreachable_field<T>(field: TagField) -> Result<T, TagError> {
    Err(TagError::Write(format!("no key for {field:?} in this format")))
}

// ── MP4 ilst ──────────────────────────────────────────────────────────────────

const COVR: [u8; 4] = *b"covr";
const GEN: [u8; 4] = *b"\xA9gen";

/// Whether the writer replaces the atom: every four-character atom the reader maps (and
/// `©gen`), and the managed freeform names under the iTunes mean, in any letter case.
fn is_managed_atom(ident: &AtomIdent<'_>) -> bool {
    match ident {
        AtomIdent::Fourcc(fourcc) => *fourcc == GEN || fourcc_field(fourcc).is_some(),
        AtomIdent::Freeform { mean, name } => mean == ITUNES_MEAN && freeform_field(name).is_some(),
    }
}

fn write_ilst(
    ilst: &mut Ilst,
    current: &FileTags,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
) -> Result<(), TagError> {
    // lofty 0.25.4 panics (an unfinished `AtomWriterCompanion` asserts on drop) when it
    // can't encode an ilst, and a `covr` image type it doesn't know is such a case.
    if let Some(cover) = &tags.cover
        && !matches!(
            cover.mime.as_str(),
            "image/jpeg" | "image/png" | "image/bmp" | "image/gif" | UNKNOWN_MIME
        )
    {
        return Err(TagError::Write(format!("an MP4 cover can't be {}", cover.mime)));
    }
    let atoms = values
        .iter()
        .map(|(field, values)| ilst_atoms(*field, values, opts))
        .collect::<Result<Vec<_>, _>>()?;
    let dropped = dropped_extras(current, tags);
    ilst.retain(|atom| {
        let dropped_extra = matches!(atom.ident(), AtomIdent::Freeform { name, .. } if dropped.contains(name.as_ref()));
        !is_managed_atom(atom.ident()) && !dropped_extra
    });
    if tags.cover != current.cover {
        set_ilst_cover(ilst, tags.cover.as_ref());
    }
    let extras = added_extras(current, tags, AudioFormat::Mp4)?
        .into_iter()
        .filter_map(|(key, values)| freeform(Cow::Owned(key), &values));
    for atom in atoms.into_iter().flatten().chain(extras) {
        ilst.insert(atom);
    }
    Ok(())
}

/// Replaces or removes the cover among the `covr` atom's pictures, keeping the others in order.
fn set_ilst_cover(ilst: &mut Ilst, cover: Option<&Cover>) {
    let mut data: Vec<AtomData> = ilst
        .remove(&AtomIdent::Fourcc(COVR))
        .flat_map(Atom::into_data)
        .collect();
    let pictures: Vec<usize> = data
        .iter()
        .enumerate()
        .filter_map(|(i, d)| matches!(d, AtomData::Picture(_)).then_some(i))
        .collect();
    let at = cover_position(data.iter().filter_map(|d| match d {
        AtomData::Picture(p) => Some(p),
        _ => None,
    }))
    .and_then(|n| pictures.get(n).copied());
    // MP4 pictures carry no type; lofty files them all as `Other`.
    let new = cover.map(|c| AtomData::Picture(picture(c, PictureType::Other)));
    match (at, new) {
        (Some(i), Some(new)) => data[i] = new,
        (Some(i), None) => {
            data.remove(i);
        }
        (None, Some(new)) => data.push(new),
        (None, None) => {}
    }
    if let Some(atom) = Atom::from_collection(AtomIdent::Fourcc(COVR), data) {
        ilst.insert(atom);
    }
}

/// The atoms `field` is written as. Genre goes in `©gen` (joined, for players that read
/// only that) and in the freeform GENRE, one data entry per value.
fn ilst_atoms(field: TagField, values: &[String], opts: &WriteOptions) -> Result<Vec<Atom<'static>>, TagError> {
    let text = |fourcc: [u8; 4]| Atom::new(AtomIdent::Fourcc(fourcc), AtomData::UTF8(values.join(JOIN)));
    Ok(match field {
        TagField::Title => vec![text(*b"\xA9nam")],
        TagField::Artist => vec![text(*b"\xA9ART")],
        TagField::AlbumArtist => vec![text(*b"aART")],
        TagField::Album => vec![text(*b"\xA9alb")],
        TagField::Composer => vec![text(*b"\xA9wrt")],
        TagField::Date => vec![text(*b"\xA9day")],
        TagField::Comment => vec![text(*b"\xA9cmt")],
        TagField::Track => vec![number_atom(*b"trkn", &values.join(JOIN), opts.track_total, 8)?],
        TagField::Disc => vec![number_atom(*b"disk", &values.join(JOIN), opts.disc_total, 6)?],
        TagField::Genre => std::iter::once(text(GEN))
            .chain(freeform_atom(field, values)?)
            .collect(),
        TagField::Performer | TagField::Publisher | TagField::Translator | TagField::Description | TagField::Www => {
            freeform_atom(field, values)?.into_iter().collect()
        }
    })
}

/// A freeform atom under the iTunes mean, one data entry per value (`None` for no values).
fn freeform_atom(field: TagField, values: &[String]) -> Result<Option<Atom<'static>>, TagError> {
    let name = FREEFORM_FIELDS
        .iter()
        .find(|(_, f)| *f == field)
        .map_or_else(|| unreachable_field(field), |(name, _)| Ok(*name))?;
    Ok(freeform(Cow::Borrowed(name), values))
}

/// A freeform atom `name` under the iTunes mean, one UTF-8 data entry per value (`None` for
/// no values).
fn freeform(name: Cow<'static, str>, values: &[String]) -> Option<Atom<'static>> {
    let ident = AtomIdent::Freeform {
        mean: Cow::Borrowed(ITUNES_MEAN),
        name,
    };
    let data = values.iter().map(|v| AtomData::UTF8(v.clone())).collect();
    Atom::from_collection(ident, data)
}

/// A `trkn` / `disk` atom: `len` bytes, the number in bytes 2-3 and the total in 4-5.
fn number_atom(fourcc: [u8; 4], value: &str, total: Option<u32>, len: usize) -> Result<Atom<'static>, TagError> {
    let number = as_u16(&number_text(value, None))?;
    let total = total.map_or(Ok(0), |t| as_u16(&t.to_string()))?;
    let mut data = vec![0u8; len];
    data[2..4].copy_from_slice(&number.to_be_bytes());
    data[4..6].copy_from_slice(&total.to_be_bytes());
    let data = AtomData::Unknown {
        code: DataType::Reserved,
        data,
    };
    Ok(Atom::new(AtomIdent::Fourcc(fourcc), data))
}

// ── Vorbis comments (FLAC, Ogg Vorbis, Opus) ──────────────────────────────────

/// `comments` with the write applied (pictures included, which by now all live in the
/// comments), and what the saved file must read back as.
fn write_vorbis_family(
    comments: &VorbisComments,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
    format: AudioFormat,
) -> Result<(VorbisComments, FileTags), TagError> {
    let mut comments = comments.clone();
    let current = read_vorbis(&comments, comments.pictures().iter().map(|(p, _)| p));
    refuse_loss(hidden_items(&vorbis_items(&comments), &current), opts.allow_frame_loss)?;
    write_vorbis(&mut comments, &current, tags, values, opts, format)?;
    if tags.cover != current.cover {
        set_ogg_cover(&mut comments, tags.cover.as_ref())?;
    }
    let edited = read_vorbis(&comments, comments.pictures().iter().map(|(p, _)| p));
    let expected = expected_tags(&current, tags, values, format, &edited)?;
    Ok((comments, expected))
}

/// Removes every managed key and total (in any letter case) and every dropped extra,
/// keeping the other items in order, then writes the managed values, one item per value,
/// `TRACKTOTAL` / `DISCTOTAL` after the track / disc when the caller knows the total, and
/// the extras the file lacks, one item per value.
fn write_vorbis(
    comments: &mut VorbisComments,
    current: &FileTags,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
    format: AudioFormat,
) -> Result<(), TagError> {
    let added = added_extras(current, tags, format)?;
    let dropped = dropped_extras(current, tags);
    let kept: Vec<(String, String)> = comments
        .take_items()
        .filter(|(key, _)| {
            let key = key.to_ascii_uppercase();
            let managed = vorbis_field(&key).is_some() || VORBIS_TOTALS.contains(&key.as_str());
            !managed && !dropped.contains(&key)
        })
        .collect();
    for (key, value) in kept {
        comments.push(key, value);
    }
    for (key, field) in VORBIS_FIELDS {
        let Some(values) = values.get(&field) else { continue };
        for value in values {
            let value = match field {
                TagField::Track | TagField::Disc => number_text(value, None),
                _ => value.clone(),
            };
            comments.push(key.to_owned(), value);
        }
        let total = match field {
            TagField::Track => opts.track_total.map(|t| ("TRACKTOTAL", t)),
            TagField::Disc => opts.disc_total.map(|t| ("DISCTOTAL", t)),
            _ => None,
        };
        if let Some((key, total)) = total {
            comments.push(key.to_owned(), total.to_string());
        }
    }
    for (key, values) in added {
        for value in values {
            comments.push(key.clone(), value);
        }
    }
    Ok(())
}

/// Replaces (as a front cover) or removes the cover among `storage`'s pictures.
fn set_ogg_cover(storage: &mut impl OggPictureStorage, cover: Option<&Cover>) -> Result<(), TagError> {
    let at = cover_position(storage.pictures().iter().map(|(p, _)| p));
    let new = cover.map(|c| {
        let picture = picture(c, PictureType::CoverFront);
        // Dimensions are informational; an image lofty can't measure is written with them
        // unknown (zero), as the format allows.
        let info = PictureInformation::from_picture(&picture).unwrap_or_default();
        (picture, info)
    });
    match (at, new) {
        (Some(i), Some((picture, info))) => storage.set_picture(i, picture, info),
        (Some(i), None) => {
            storage.remove_picture(i);
        }
        (None, Some((picture, info))) => {
            storage.insert_picture(picture, Some(info)).map_err(write_error)?;
        }
        (None, None) => {}
    }
    Ok(())
}
