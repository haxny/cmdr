//! The tag fields the generic `lofty` tag can't answer the way the Tags window does:
//! performer, composer, publisher, the stored date, totals, description, translator and
//! web address. They live in format-specific places (`TXXX:PERFORMER`, a freeform
//! `PERFORMER` atom, a `PERFORMER` Vorbis comment), so this reads each format's own tag.
//!
//! The maps mirror `cmdr-tags`' reader (`frame_field`, `txxx_field`, `fourcc_field`,
//! `freeform_field`, `vorbis_field`, `totals.rs`), so a mask and the Tags window agree on
//! the same file. ❌ Changing one without the other makes them disagree.

use std::io::{Read, Seek, SeekFrom};

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::mp4::{AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OpusFile, VorbisFile};

use crate::types::{MediaFacts, ReadError};

/// The separator the Tags window joins multiple values with where a format keeps one string.
const JOIN: &str = "; ";
/// The `mean` of the freeform MP4 atoms the Tags window manages.
const ITUNES_MEAN: &str = "com.apple.iTunes";

const TRKN: [u8; 4] = *b"trkn";
const DISK: [u8; 4] = *b"disk";

/// The fields this module fills, one value list each.
#[derive(Clone, Copy)]
enum Kind {
    Performer,
    Composer,
    Publisher,
    Date,
    Translator,
    Description,
    Www,
}

/// The new tag fields of the file of `file_type` in `reader`, as a `MediaFacts` holding only
/// them. A format with none of these places (APE, RIFF, AIFF) gives nothing.
pub(crate) fn read_native<R: Read + Seek>(reader: &mut R, file_type: FileType) -> Result<MediaFacts, ReadError> {
    reader.seek(SeekFrom::Start(0)).map_err(ReadError::from_io)?;
    let options = ParseOptions::new().read_properties(false).read_cover_art(false);
    let mut vorbis_options = options;
    vorbis_options.implicit_conversions(false);
    let mut facts = MediaFacts::default();
    match file_type {
        FileType::Mpeg => {
            if let Some(id3) = MpegFile::read_from(reader, options).map_err(parsed)?.id3v2() {
                from_id3(&mut facts, id3);
            }
        }
        FileType::Mp4 => {
            if let Some(ilst) = Mp4File::read_from(reader, options).map_err(parsed)?.ilst() {
                from_ilst(&mut facts, ilst);
            }
        }
        // Without implicit conversions lofty keeps `3/12` in `TRACKNUMBER`, as the Tags window reads it.
        FileType::Flac => {
            let file = lofty::flac::FlacFile::read_from(reader, vorbis_options).map_err(parsed)?;
            if let Some(comments) = file.vorbis_comments() {
                from_vorbis(&mut facts, comments);
            }
        }
        FileType::Vorbis => {
            let file = VorbisFile::read_from(reader, vorbis_options).map_err(parsed)?;
            from_vorbis(&mut facts, file.vorbis_comments());
        }
        FileType::Opus => {
            let file = OpusFile::read_from(reader, vorbis_options).map_err(parsed)?;
            from_vorbis(&mut facts, file.vorbis_comments());
        }
        _ => {}
    }
    Ok(facts)
}

/// A lofty parse error as the typed one, so a stopped read keeps its reason.
fn parsed(error: impl std::error::Error + 'static) -> ReadError {
    ReadError::from_error(&error)
}

/// Puts `values` (trimmed, empties gone) in `kind`'s field of `facts`. A single-value field
/// keeps the first value ever put; a multi-value one collects them, each once.
fn put(facts: &mut MediaFacts, kind: Kind, values: impl IntoIterator<Item = String>) {
    let is_multi = matches!(kind, Kind::Performer | Kind::Composer | Kind::Translator | Kind::Www);
    let values: Vec<String> = values
        .into_iter()
        // Only a multi-value field holds several values joined with `; `, as in cmdr-tags.
        .flat_map(|value| {
            if is_multi {
                value.split(JOIN).map(str::to_owned).collect()
            } else {
                vec![value]
            }
        })
        .map(|value: String| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    let multi = match kind {
        Kind::Performer => &mut facts.performer,
        Kind::Composer => &mut facts.composer,
        Kind::Translator => &mut facts.translator,
        Kind::Www => &mut facts.www,
        Kind::Publisher | Kind::Date | Kind::Description => {
            let single = match kind {
                Kind::Publisher => &mut facts.publisher,
                Kind::Date => &mut facts.date,
                _ => &mut facts.description,
            };
            if single.is_none() {
                *single = values.into_iter().next();
            }
            return;
        }
    };
    for value in values {
        if !multi.contains(&value) {
            multi.push(value);
        }
    }
}

/// `text` split on NULs, ID3's value separator.
fn split_nulls(text: &str) -> Vec<String> {
    text.split('\0').map(str::to_owned).collect()
}

/// The total in a positive whole number's text, `None` for zero, blank or not a number.
fn total_of(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok().filter(|total| *total > 0)
}

/// The total in an `N/T` text, when there's a number before the slash.
fn slash_total(text: &str) -> Option<i64> {
    let (number, total) = text.split_once('/')?;
    (!number.trim().is_empty()).then(|| total_of(total)).flatten()
}

fn from_id3(facts: &mut MediaFacts, id3: &Id3v2Tag) {
    let mut legacy_disc_total = None;
    for frame in id3 {
        match frame {
            Frame::Text(f) => {
                let kind = match f.id().as_str() {
                    "TCOM" => Kind::Composer,
                    "TPUB" => Kind::Publisher,
                    "TDRC" | "TYER" => Kind::Date,
                    "TRCK" => {
                        facts.total_tracks = facts.total_tracks.or_else(|| slash_total(&f.value));
                        continue;
                    }
                    "TPOS" => {
                        facts.total_discs = facts.total_discs.or_else(|| slash_total(&f.value));
                        continue;
                    }
                    _ => continue,
                };
                put(facts, kind, split_nulls(&f.value));
            }
            Frame::Timestamp(f) => {
                if matches!(f.id().as_str(), "TDRC" | "TYER") {
                    put(facts, Kind::Date, [f.timestamp.to_string()]);
                }
            }
            Frame::UserText(f) => {
                let description = f.description.as_ref();
                let kind = [
                    ("Performer", Kind::Performer),
                    ("Translator", Kind::Translator),
                    ("Description", Kind::Description),
                    ("www", Kind::Www),
                ]
                .into_iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(description))
                .map(|(_, kind)| kind);
                if let Some(kind) = kind {
                    put(facts, kind, split_nulls(&f.content));
                } else if description.eq_ignore_ascii_case("DiscNumber") {
                    legacy_disc_total = legacy_disc_total.or_else(|| slash_total(&f.content));
                }
            }
            _ => {}
        }
    }
    // The legacy disc frame only fills a total `TPOS` left empty.
    facts.total_discs = facts.total_discs.or(legacy_disc_total);
}

fn from_ilst(facts: &mut MediaFacts, ilst: &Ilst) {
    let texts = |atom: &lofty::mp4::Atom<'_>| -> Vec<String> {
        atom.data()
            .filter_map(|data| match data {
                AtomData::UTF8(text) | AtomData::UTF16(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    for atom in ilst {
        match atom.ident() {
            AtomIdent::Fourcc([0xA9, b'w', b'r', b't']) => put(facts, Kind::Composer, texts(atom)),
            AtomIdent::Fourcc([0xA9, b'd', b'a', b'y']) => put(facts, Kind::Date, texts(atom)),
            AtomIdent::Fourcc(fourcc @ (TRKN | DISK)) => {
                // Bytes 2-3 are the number, 4-5 the total; zero means none.
                if let Some(AtomData::Unknown { data, .. }) = atom.data().next()
                    && data.len() >= 6
                    && u16::from_be_bytes([data[2], data[3]]) > 0
                {
                    let total = Some(i64::from(u16::from_be_bytes([data[4], data[5]]))).filter(|t| *t > 0);
                    let slot = if *fourcc == TRKN {
                        &mut facts.total_tracks
                    } else {
                        &mut facts.total_discs
                    };
                    *slot = slot.or(total);
                }
            }
            AtomIdent::Freeform { mean, name } if mean == ITUNES_MEAN => {
                let kind = [
                    ("PERFORMER", Kind::Performer),
                    ("publisher", Kind::Publisher),
                    ("www", Kind::Www),
                    ("translator", Kind::Translator),
                    ("description", Kind::Description),
                ]
                .into_iter()
                .find(|(managed, _)| managed.eq_ignore_ascii_case(name))
                .map(|(_, kind)| kind);
                if let Some(kind) = kind {
                    put(facts, kind, texts(atom));
                }
            }
            _ => {}
        }
    }
}

fn from_vorbis(facts: &mut MediaFacts, comments: &VorbisComments) {
    let mut track_number = false;
    let mut disc_number = false;
    let mut slashed = (None, None);
    for (key, value) in comments.items() {
        let key = key.to_ascii_uppercase();
        let kind = match key.as_str() {
            "PERFORMER" => Kind::Performer,
            "COMPOSER" => Kind::Composer,
            "PUBLISHER" => Kind::Publisher,
            "DATE" => Kind::Date,
            "TRANSLATOR" => Kind::Translator,
            "DESCRIPTION" => Kind::Description,
            "WWW" => Kind::Www,
            "TRACKTOTAL" | "TOTALTRACKS" => {
                facts.total_tracks = facts.total_tracks.or_else(|| total_of(value));
                continue;
            }
            "DISCTOTAL" | "TOTALDISCS" => {
                facts.total_discs = facts.total_discs.or_else(|| total_of(value));
                continue;
            }
            "TRACKNUMBER" => {
                track_number |= !value.trim().is_empty();
                slashed.0 = slashed.0.or_else(|| slash_total(value));
                continue;
            }
            "DISCNUMBER" => {
                disc_number |= !value.trim().is_empty();
                slashed.1 = slashed.1.or_else(|| slash_total(value));
                continue;
            }
            _ => continue,
        };
        put(facts, kind, [value.to_owned()]);
    }
    // A total key wins over `N/T`.
    facts.total_tracks = facts.total_tracks.filter(|_| track_number).or(slashed.0);
    facts.total_discs = facts.total_discs.filter(|_| disc_number).or(slashed.1);
}
