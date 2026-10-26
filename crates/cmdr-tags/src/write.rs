//! Write the managed fields and the cover into an audio file, and remove the extras the
//! caller dropped, leaving every other frame, atom, and key as it was. One private function
//! per tag container, all saved through [`save_safely`].
//!
//! Each writer is the inverse of its reader in `read.rs`: it removes everything the reader
//! maps to a managed field (aliases and duplicates included), then writes one frame, atom,
//! or key per field.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{ErrorKind, Seek};
use std::path::Path;

use lofty::TextEncoding;
use lofty::config::WriteOptions as LoftyWriteOptions;
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

use crate::format::AudioFormat;
use crate::model::{ContentKind, Cover, FileTags, TagError, TagField};
use crate::read::{
    FREEFORM_FIELDS, ITUNES_MEAN, JOIN, TXXX_FIELDS, UNKNOWN_MIME, VORBIS_FIELDS, cover_position, format_of_path,
    fourcc_field, frame_field, freeform_field, id3_pictures, parse_error, parse_options, read_id3, read_ilst,
    read_vorbis, txxx_field,
};

/// How a write maps fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Audiobooks drop the composer (and ID3's `TPE3`) whatever `FileTags` holds.
    pub kind: ContentKind,
    /// The number of tracks, when the caller knows it: the track is then written `N/Total`,
    /// otherwise plain `N`.
    pub track_total: Option<u32>,
}

/// Writes `tags` into the audio file at `path` with a safe overwrite. Afterwards the file
/// holds exactly `tags.fields` for the managed fields (an absent field is removed),
/// `tags.cover` as the cover (`None` removes it; other pictures stay), and only those
/// extras of the file that `tags.extras` still has a key for. Extras are never written:
/// one absent from `tags.extras` is removed, one present is left as the file has it.
/// Everything else in the file is untouched.
pub fn write_tags(path: &Path, tags: &FileTags, opts: &WriteOptions) -> Result<(), TagError> {
    let format = format_of_path(path)?;
    let values = values_to_write(tags, opts.kind);
    let mut source = File::open(path)?;
    let defaults = LoftyWriteOptions::default();
    match format {
        AudioFormat::Mp3 => {
            let file = MpegFile::read_from(&mut source, parse_options()).map_err(parse_error)?;
            let mut id3 = file.id3v2().cloned().unwrap_or_default();
            write_id3(&mut id3, tags, &values, opts)?;
            save_safely(path, |out| {
                save(&id3, out, LoftyWriteOptions::new().use_id3v23(true), format)
            })
        }
        AudioFormat::Mp4 => {
            let file = Mp4File::read_from(&mut source, parse_options()).map_err(parse_error)?;
            let mut ilst = file.ilst().cloned().unwrap_or_default();
            write_ilst(&mut ilst, tags, &values, opts)?;
            save_safely(path, |out| save(&ilst, out, defaults, format))
        }
        AudioFormat::Flac => {
            let mut file = FlacFile::read_from(&mut source, parse_options()).map_err(parse_error)?;
            let mut comments = file.vorbis_comments().cloned().unwrap_or_default();
            let current = read_vorbis(&comments, file.pictures());
            write_vorbis(&mut comments, &current, tags, &values, opts);
            set_ogg_cover(&mut file, tags.cover.as_ref())?;
            // Saving the comments rewrites every FLAC picture block from the comments' own
            // pictures, so all of them move there, the block pictures first: the reader then
            // finds the same cover.
            let in_tag = comments.remove_pictures();
            for (picture, info) in file.remove_pictures().into_iter().chain(in_tag) {
                comments.insert_picture(picture, Some(info)).map_err(write_error)?;
            }
            save_safely(path, |out| save(&comments, out, defaults, format))
        }
        AudioFormat::OggVorbis => {
            let file = VorbisFile::read_from(&mut source, parse_options()).map_err(parse_error)?;
            let comments = write_ogg(file.vorbis_comments(), tags, &values, opts)?;
            save_safely(path, |out| save(&comments, out, defaults, format))
        }
        AudioFormat::Opus => {
            let file = OpusFile::read_from(&mut source, parse_options()).map_err(parse_error)?;
            let comments = write_ogg(file.vorbis_comments(), tags, &values, opts)?;
            save_safely(path, |out| save(&comments, out, defaults, format))
        }
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// The managed values a write puts in the file, by field: trimmed, empties dropped, one
/// value for a single-value field, no Composer for audiobooks. A field missing here is
/// removed from the file.
type Values = BTreeMap<TagField, Vec<String>>;

fn values_to_write(tags: &FileTags, kind: ContentKind) -> Values {
    tags.fields
        .iter()
        .filter(|(field, _)| !(kind == ContentKind::Audiobook && **field == TagField::Composer))
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

/// The keys of the extras the file has (as the reader reports them) that `tags` dropped.
fn dropped_extras(current: &FileTags, tags: &FileTags) -> BTreeSet<String> {
    current
        .extras
        .keys()
        .filter(|key| !tags.extras.contains_key(*key))
        .cloned()
        .collect()
}

/// A track or disc number as text: the number before any `/`, plus `/total` when known.
fn number_text(value: &str, total: Option<u32>) -> String {
    let number = value.split('/').next().unwrap_or_default().trim();
    match total {
        Some(total) => format!("{number}/{total}"),
        None => number.to_owned(),
    }
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

/// Saves `tag` into `out`, then parses `out` back as `format`. lofty has corrupted a
/// container on save (0.25.4 truncates a FLAC whose metadata runs to the end of the file
/// when the metadata shrinks); a copy that no longer parses never replaces the original.
fn save<T: TagExt>(tag: &T, out: &mut File, options: LoftyWriteOptions, format: AudioFormat) -> Result<(), TagError> {
    tag.save_to(out, options).map_err(write_error)?;
    out.rewind()?;
    let parsed = match format {
        AudioFormat::Mp3 => MpegFile::read_from(out, parse_options()).map(drop),
        AudioFormat::Mp4 => Mp4File::read_from(out, parse_options()).map(drop),
        AudioFormat::Flac => FlacFile::read_from(out, parse_options()).map(drop),
        AudioFormat::OggVorbis => VorbisFile::read_from(out, parse_options()).map(drop),
        AudioFormat::Opus => OpusFile::read_from(out, parse_options()).map(drop),
    };
    parsed.map_err(|e| TagError::Write(format!("the written file doesn't parse: {e}")))
}

// ── Safe overwrite ────────────────────────────────────────────────────────────

/// Copies the file at `path` to a temp file in the same directory, lets `save` write the
/// tags into the copy, fsyncs it, and renames it over the original. Any failure leaves the
/// original as it was and removes the temp file. The copy keeps the original's permissions
/// (and, on macOS, its extended attributes such as Finder tags); a symlink is followed, so
/// the link stays a link. A read-only file is refused.
pub(crate) fn save_safely(path: &Path, save: impl FnOnce(&mut File) -> Result<(), TagError>) -> Result<(), TagError> {
    let target = fs::canonicalize(path)?;
    let dir = target
        .parent()
        .ok_or_else(|| TagError::Write("the file has no parent directory".to_owned()))?;
    let permissions = fs::metadata(&target)?.permissions();
    if permissions.readonly() {
        return Err(TagError::Io(
            ErrorKind::PermissionDenied,
            "the file is read-only".to_owned(),
        ));
    }
    let mut temp = tempfile::Builder::new()
        .prefix(".cmdr-tags-")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    // `fs::copy` carries permissions and, on macOS (`fcopyfile`), extended attributes.
    fs::copy(&target, temp.path())?;
    let file = temp.as_file_mut();
    file.rewind()?;
    save(file)?;
    file.set_permissions(permissions)?;
    file.sync_all()?;
    temp.persist(&target).map_err(|e| TagError::from(e.error))?;
    // The rename itself is durable only once the directory is synced.
    File::open(dir)?.sync_all()?;
    Ok(())
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

fn write_id3(id3: &mut Id3v2Tag, tags: &FileTags, values: &Values, opts: &WriteOptions) -> Result<(), TagError> {
    let frames = values
        .iter()
        .map(|(field, values)| id3_frame(*field, values, opts.track_total))
        .collect::<Result<Vec<_>, _>>()?;
    let dropped = dropped_extras(&read_id3(id3), tags);
    let cover_at = cover_position(id3_pictures(id3));
    let mut picture_index = 0;
    id3.retain(|frame| match frame {
        Frame::Picture(_) => {
            picture_index += 1;
            Some(picture_index - 1) != cover_at
        }
        Frame::UserText(f) if dropped.contains(f.description.as_ref()) => false,
        _ => !is_managed_id3(frame, opts.kind),
    });
    for frame in frames {
        id3.insert(frame);
    }
    if let Some(cover) = &tags.cover {
        let frame = AttachedPictureFrame::new(TextEncoding::UTF8, picture(cover, PictureType::CoverFront));
        id3.insert(Frame::Picture(frame));
    }
    Ok(())
}

/// The one frame `field` is written as. Text is UTF-8 here; lofty re-encodes it as UTF-16
/// for ID3v2.3.
fn id3_frame(field: TagField, values: &[String], track_total: Option<u32>) -> Result<Frame<'static>, TagError> {
    let joined = values.join(JOIN);
    let text = |id: &'static str, value: String| {
        Frame::Text(TextInformationFrame::new(
            FrameId::Valid(Cow::Borrowed(id)),
            TextEncoding::UTF8,
            value,
        ))
    };
    Ok(match field {
        TagField::Title => text("TIT2", joined),
        TagField::Artist => text("TPE1", joined),
        TagField::AlbumArtist => text("TPE2", joined),
        TagField::Album => text("TALB", joined),
        TagField::Composer => text("TCOM", joined),
        TagField::Genre => text("TCON", joined),
        TagField::Publisher => text("TPUB", joined),
        TagField::Track => text("TRCK", number_text(&joined, track_total)),
        TagField::Disc => text("TPOS", number_text(&joined, None)),
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

fn write_ilst(ilst: &mut Ilst, tags: &FileTags, values: &Values, opts: &WriteOptions) -> Result<(), TagError> {
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
        .map(|(field, values)| ilst_atoms(*field, values, opts.track_total))
        .collect::<Result<Vec<_>, _>>()?;
    let dropped = dropped_extras(&read_ilst(ilst), tags);
    ilst.retain(|atom| {
        let dropped_extra = matches!(atom.ident(), AtomIdent::Freeform { name, .. } if dropped.contains(name.as_ref()));
        !is_managed_atom(atom.ident()) && !dropped_extra
    });
    set_ilst_cover(ilst, tags.cover.as_ref());
    for atom in atoms.into_iter().flatten() {
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
fn ilst_atoms(field: TagField, values: &[String], track_total: Option<u32>) -> Result<Vec<Atom<'static>>, TagError> {
    let text = |fourcc: [u8; 4]| Atom::new(AtomIdent::Fourcc(fourcc), AtomData::UTF8(values.join(JOIN)));
    Ok(match field {
        TagField::Title => vec![text(*b"\xA9nam")],
        TagField::Artist => vec![text(*b"\xA9ART")],
        TagField::AlbumArtist => vec![text(*b"aART")],
        TagField::Album => vec![text(*b"\xA9alb")],
        TagField::Composer => vec![text(*b"\xA9wrt")],
        TagField::Date => vec![text(*b"\xA9day")],
        TagField::Comment => vec![text(*b"\xA9cmt")],
        TagField::Track => vec![number_atom(*b"trkn", &values.join(JOIN), track_total, 8)?],
        TagField::Disc => vec![number_atom(*b"disk", &values.join(JOIN), None, 6)?],
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
    let ident = AtomIdent::Freeform {
        mean: Cow::Borrowed(ITUNES_MEAN),
        name: Cow::Borrowed(name),
    };
    let data = values.iter().map(|v| AtomData::UTF8(v.clone())).collect();
    Ok(Atom::from_collection(ident, data))
}

/// A `trkn` / `disk` atom: `len` bytes, the number in bytes 2-3 and the total in 4-5.
fn number_atom(fourcc: [u8; 4], value: &str, total: Option<u32>, len: usize) -> Result<Atom<'static>, TagError> {
    let as_u16 = |text: &str| {
        text.trim()
            .parse::<u16>()
            .map_err(|e| TagError::Write(format!("{text:?} isn't a track or disc number: {e}")))
    };
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

/// Ogg Vorbis and Opus keep their pictures in the comments.
fn write_ogg(
    comments: &VorbisComments,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
) -> Result<VorbisComments, TagError> {
    let mut comments = comments.clone();
    let current = read_vorbis(&comments, comments.pictures());
    write_vorbis(&mut comments, &current, tags, values, opts);
    set_ogg_cover(&mut comments, tags.cover.as_ref())?;
    Ok(comments)
}

/// Removes every managed key (in any letter case) and every dropped extra, keeping the
/// other items in order, then writes the managed values, one item per value.
fn write_vorbis(
    comments: &mut VorbisComments,
    current: &FileTags,
    tags: &FileTags,
    values: &Values,
    opts: &WriteOptions,
) {
    let dropped = dropped_extras(current, tags);
    let kept: Vec<(String, String)> = comments
        .take_items()
        .filter(|(key, _)| {
            let key = key.to_ascii_uppercase();
            VORBIS_FIELDS.iter().all(|(name, _)| *name != key) && !dropped.contains(&key)
        })
        .collect();
    for (key, value) in kept {
        comments.push(key, value);
    }
    for (key, field) in VORBIS_FIELDS {
        let Some(values) = values.get(&field) else { continue };
        for value in values {
            let value = match field {
                TagField::Track => number_text(value, opts.track_total),
                TagField::Disc => number_text(value, None),
                _ => value.clone(),
            };
            comments.push(key.to_owned(), value);
        }
    }
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
