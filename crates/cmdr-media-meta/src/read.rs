//! [`read_media`]: one file's facts, by extension and then content.

use std::io::{Read, Seek, SeekFrom};

use lofty::config::ParseOptions;
use lofty::file::{FileType, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};

use crate::clock::{ToLocal, system_local};
use crate::source::{BlockReader, ByteSource};
use crate::types::{MediaFacts, ReadError};
use crate::{ebml, holes, isobmff, native, photo};

/// Which reader a file's extension sends it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// EXIF, through our container walks and `kamadak-exif`.
    Photo,
    /// Tags, through `lofty`.
    Audio,
    /// MP4 / MOV: our box walk for the capture date, then `lofty` for tags.
    Mp4Video,
    /// MKV / WebM / MKA: our EBML walk.
    Matroska,
}

/// The reader for `file_name`, or `None` for a file no reader takes (CR3 and RAF
/// among them: `is_media_name` counts them, so they read as `NotSupported`).
fn format_of(file_name: &str) -> Option<Format> {
    let extension = match file_name.rfind('.') {
        Some(dot) if dot > 0 => file_name[dot + 1..].to_ascii_lowercase(),
        _ => return None,
    };
    let format = match extension.as_str() {
        "jpg" | "jpeg" | "heic" | "heif" | "tif" | "tiff" | "png" | "webp" | "dng" | "cr2" | "nef" | "arw" | "orf"
        | "rw2" => Format::Photo,
        "mp3" | "m4a" | "m4b" | "flac" | "ogg" | "oga" | "opus" | "wav" | "aif" | "aiff" | "ape" | "wv" | "mpc"
        | "aac" => Format::Audio,
        "mp4" | "mov" | "m4v" | "3gp" => Format::Mp4Video,
        "mkv" | "webm" | "mka" => Format::Matroska,
        _ => return None,
    };
    Some(format)
}

/// Every field `file_name`'s bytes in `reader` carry. Dispatches by extension
/// (`is_media_name`'s list): photos to our EXIF walks, audio to `lofty`, MP4/MOV
/// to our box walk then `lofty`, MKV/WebM to our EBML walk. A field the file
/// doesn't have is `None`; a refusal the reader raises (`Cancelled`, `TooLarge`,
/// …) comes back as itself.
pub fn read_media<R: Read + Seek>(reader: &mut R, file_name: &str) -> Result<MediaFacts, ReadError> {
    read_media_in(reader, file_name, &system_local)
}

/// [`read_media`] over `source`, through a [`BlockReader`] that never fetches the
/// file's cover art: a pre-pass maps it to holes first. The loader's entry point.
pub fn read_media_from<S: ByteSource>(source: S, file_name: &str) -> Result<MediaFacts, ReadError> {
    let Some(format) = format_of(file_name) else {
        return Err(ReadError::NotSupported);
    };
    let mut reader = BlockReader::new(source);
    let holes = match format {
        Format::Audio | Format::Mp4Video => holes::cover_art(&mut reader, file_name).map_err(ReadError::from_io)?,
        Format::Photo | Format::Matroska => Vec::new(),
    };
    let mut reader = reader.with_holes(holes);
    reader.seek(SeekFrom::Start(0)).map_err(ReadError::from_io)?;
    read_media(&mut reader, file_name)
}

/// [`read_media`] with the zone injected, for tests.
pub(crate) fn read_media_in<R: Read + Seek>(
    reader: &mut R,
    file_name: &str,
    to_local: ToLocal<'_>,
) -> Result<MediaFacts, ReadError> {
    match format_of(file_name).ok_or(ReadError::NotSupported)? {
        Format::Photo => photo::read_photo(reader),
        Format::Audio => read_tags(reader, file_name),
        Format::Mp4Video => {
            let mut facts = isobmff::read_video(reader, to_local).map_err(ReadError::from_io)?;
            // A video's tags are a bonus: a MOV lofty can't parse, or whose tags
            // run past the byte budget, still has its date. Only a stopped read
            // (cancelled or out of time) ends it.
            match read_tags(reader, file_name) {
                Ok(tags) => merge_tags(&mut facts, tags),
                Err(stopped @ (ReadError::Cancelled | ReadError::TimedOut)) => return Err(stopped),
                Err(skipped) => {
                    log::debug!(target: "cmdr_media_meta::read", "No tags in a video: {skipped:?}");
                }
            }
            Ok(facts)
        }
        Format::Matroska => ebml::read_matroska(reader, to_local).map_err(ReadError::from_io),
    }
}

/// The tag fields `lofty` finds. The file's primary tag answers first, then any
/// other (an ID3v1 beside an ID3v2, say) fills what it lacks.
fn read_tags<R: Read + Seek>(reader: &mut R, file_name: &str) -> Result<MediaFacts, ReadError> {
    reader.seek(SeekFrom::Start(0)).map_err(ReadError::from_io)?;
    let options = ParseOptions::new().read_properties(false).read_cover_art(false);
    let mut probe = Probe::new(&mut *reader)
        .options(options)
        .guess_file_type()
        .map_err(ReadError::from_io)?;
    if probe.file_type().is_none() {
        // lofty knows MP4's extensions but not QuickTime's or 3GP's.
        let extension = file_name.rsplit_once('.').map_or("", |(_, ext)| ext);
        let video = format_of(file_name) == Some(Format::Mp4Video);
        let Some(file_type) = FileType::from_ext(extension).or(video.then_some(FileType::Mp4)) else {
            return Err(ReadError::NotSupported);
        };
        probe = probe.set_file_type(file_type);
    }
    let file_type = probe.file_type();
    let tagged = probe.read().map_err(|error| ReadError::from_error(&error))?;
    // The performer, publisher, totals and the rest sit where only the format's own tag shows
    // them, and it reads artist, album artist and genre as the Tags window does (every value, an
    // M4A's freeform `GENRE`). So it goes first, and the generic fields fill what it lacks. A
    // stopped read ends it, anything else leaves the native fields out.
    let mut facts = match file_type.map(|file_type| native::read_native(reader, file_type)) {
        Some(Ok(native)) => native,
        Some(Err(stopped @ (ReadError::Cancelled | ReadError::TimedOut))) => return Err(stopped),
        Some(Err(skipped)) => {
            log::debug!(target: "cmdr_media_meta::read", "No native tag fields: {skipped:?}");
            MediaFacts::default()
        }
        None => MediaFacts::default(),
    };
    let primary = tagged.primary_tag();
    let others = tagged
        .tags()
        .iter()
        .filter(|tag| !primary.is_some_and(|p| std::ptr::eq(p, *tag)));
    for tag in primary.into_iter().chain(others) {
        merge_tags(&mut facts, facts_from_tag(tag));
    }
    Ok(facts)
}

/// One tag's fields.
fn facts_from_tag(tag: &Tag) -> MediaFacts {
    let text = |value: Option<std::borrow::Cow<'_, str>>| value.map(|v| v.into_owned());
    let number = |value: Option<u32>| value.filter(|n| *n > 0).map(i64::from);
    MediaFacts {
        title: text(tag.title()),
        artist: text(tag.artist()).into_iter().collect(),
        album_artist: tag
            .get_string(ItemKey::AlbumArtist)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        album: text(tag.album()),
        genre: text(tag.genre()).into_iter().collect(),
        comment: text(tag.comment()),
        track: number(tag.track()),
        disc: number(tag.disk()),
        year: tag.date().map(|date| date.year).filter(|year| *year > 0).map(i64::from),
        ..MediaFacts::default()
    }
}

/// Fills `facts`' empty tag fields from `tags`.
fn merge_tags(facts: &mut MediaFacts, tags: MediaFacts) {
    let MediaFacts {
        title,
        artist,
        album_artist,
        album,
        genre,
        comment,
        track,
        disc,
        year,
        performer,
        composer,
        publisher,
        date,
        total_tracks,
        total_discs,
        description,
        translator,
        www,
        ..
    } = tags;
    facts.title = facts.title.take().or(title);
    facts.album = facts.album.take().or(album);
    facts.comment = facts.comment.take().or(comment);
    facts.track = facts.track.or(track);
    facts.disc = facts.disc.or(disc);
    facts.year = facts.year.or(year);
    facts.publisher = facts.publisher.take().or(publisher);
    facts.date = facts.date.take().or(date);
    facts.total_tracks = facts.total_tracks.or(total_tracks);
    facts.total_discs = facts.total_discs.or(total_discs);
    facts.description = facts.description.take().or(description);
    for (have, more) in [
        (&mut facts.artist, artist),
        (&mut facts.album_artist, album_artist),
        (&mut facts.genre, genre),
        (&mut facts.performer, performer),
        (&mut facts.composer, composer),
        (&mut facts.translator, translator),
        (&mut facts.www, www),
    ] {
        if have.is_empty() {
            *have = more;
        }
    }
}
