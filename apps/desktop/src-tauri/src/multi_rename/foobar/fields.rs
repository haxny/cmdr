//! The `%field%`s a foobar pattern can name: `%title%` falls back to the file name and
//! `%tracknumber%` pads to two digits, as in foobar; `%album artist%` falls back to the artist only
//! and `%year%` is the date's first four digits, Cmdr's choices (`multi_rename/DETAILS.md`). Values
//! come from the row's tags (`MediaFacts`, blanks are missing), multi-value fields joined with `, `.

use cmdr_media_meta::{MediaFacts, MediaField, MediaValue};

use super::eval::zero_padded;
use crate::multi_rename::mask::{RowFacts, RowMedia};

/// The digits foobar pads `%tracknumber%` to.
const TRACK_DIGITS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Artist,
    AlbumArtist,
    Album,
    Title,
    TrackNumber,
    DiscNumber,
    TotalTracks,
    TotalDiscs,
    Date,
    Year,
    Genre,
    Performer,
    Composer,
    Publisher,
    Comment,
    Description,
    Translator,
    Www,
    FileName,
    DirectoryName,
}

impl Field {
    /// The field `name` (between the `%`s) names: case-insensitive, spaces as typed.
    pub(super) fn parse(name: &str) -> Option<Self> {
        Some(match name.to_lowercase().as_str() {
            "artist" => Self::Artist,
            "album artist" => Self::AlbumArtist,
            "album" => Self::Album,
            "title" => Self::Title,
            "tracknumber" | "track" => Self::TrackNumber,
            "discnumber" | "disc" => Self::DiscNumber,
            "totaltracks" => Self::TotalTracks,
            "totaldiscs" => Self::TotalDiscs,
            "date" => Self::Date,
            "year" => Self::Year,
            "genre" => Self::Genre,
            "performer" => Self::Performer,
            "composer" => Self::Composer,
            "publisher" => Self::Publisher,
            "comment" => Self::Comment,
            "description" => Self::Description,
            "translator" => Self::Translator,
            "www" => Self::Www,
            "filename" => Self::FileName,
            "directoryname" => Self::DirectoryName,
            _ => return None,
        })
    }

    /// The tag the value comes from (the first one, for a field with a fallback); `None` for one
    /// read from the file's name or folder.
    pub(super) fn media_field(self) -> Option<MediaField> {
        Some(match self {
            Self::Artist => MediaField::TagArtist,
            Self::AlbumArtist => MediaField::TagAlbumArtist,
            Self::Album => MediaField::TagAlbum,
            Self::Title => MediaField::TagTitle,
            Self::TrackNumber => MediaField::TagTrack,
            Self::DiscNumber => MediaField::TagDisc,
            Self::TotalTracks => MediaField::TagTotalTracks,
            Self::TotalDiscs => MediaField::TagTotalDiscs,
            Self::Date => MediaField::TagDate,
            Self::Year => MediaField::TagYear,
            Self::Genre => MediaField::TagGenre,
            Self::Performer => MediaField::TagPerformer,
            Self::Composer => MediaField::TagComposer,
            Self::Publisher => MediaField::TagPublisher,
            Self::Comment => MediaField::TagComment,
            Self::Description => MediaField::TagDescription,
            Self::Translator => MediaField::TagTranslator,
            Self::Www => MediaField::TagWww,
            Self::FileName | Self::DirectoryName => return None,
        })
    }

    /// The field's text for `row`, or `None` when it has no value (never `Some("")`).
    pub(super) fn value(self, row: &RowFacts) -> Option<String> {
        let tags = match row.media {
            RowMedia::Read(facts) => Some(facts),
            RowMedia::Reading | RowMedia::Unreadable | RowMedia::NotMedia => None,
        };
        let tag = |field| tags.and_then(|facts| text(facts, field));
        let stem = || non_empty(row.split_name().0);
        match self {
            Self::Artist => tag(MediaField::TagArtist),
            Self::AlbumArtist => tag(MediaField::TagAlbumArtist).or_else(|| tag(MediaField::TagArtist)),
            Self::Album => tag(MediaField::TagAlbum),
            Self::Title => tag(MediaField::TagTitle).or_else(stem),
            Self::TrackNumber => match tags.and_then(|facts| facts.get(MediaField::TagTrack)) {
                Some(MediaValue::Number(track)) => Some(zero_padded(track, TRACK_DIGITS)),
                _ => tag(MediaField::TagTrack),
            },
            Self::DiscNumber => tag(MediaField::TagDisc),
            Self::TotalTracks => tag(MediaField::TagTotalTracks),
            Self::TotalDiscs => tag(MediaField::TagTotalDiscs),
            Self::Date => tag(MediaField::TagDate).or_else(|| tag(MediaField::TagYear)),
            Self::Year => tags.and_then(year),
            Self::Genre => tag(MediaField::TagGenre),
            Self::Performer => tag(MediaField::TagPerformer),
            Self::Composer => tag(MediaField::TagComposer),
            Self::Publisher => tag(MediaField::TagPublisher),
            Self::Comment => tag(MediaField::TagComment),
            Self::Description => tag(MediaField::TagDescription),
            Self::Translator => tag(MediaField::TagTranslator),
            Self::Www => tag(MediaField::TagWww),
            Self::FileName => stem(),
            Self::DirectoryName => non_empty(row.parent),
        }
    }
}

/// `field` of `facts` as text (cleaned and joined by `MediaFacts::get`).
fn text(facts: &MediaFacts, field: MediaField) -> Option<String> {
    match facts.get(field)? {
        MediaValue::Text(text) => Some(text.into_owned()),
        MediaValue::Number(number) => Some(number.to_string()),
        MediaValue::Date(date) => Some(date.format("%Y-%m-%d").to_string()),
    }
}

/// `%year%` (a Cmdr convenience; foobar reads a raw YEAR tag): the date's year, else the year the
/// reader found.
fn year(facts: &MediaFacts) -> Option<String> {
    let from_date = text(facts, MediaField::TagDate).and_then(|date| year_of(&date).map(str::to_string));
    from_date.or_else(|| text(facts, MediaField::TagYear))
}

/// A date's year, as foobar's `$year` reads it: its first four characters when they're digits
/// (`2011-05-01` → `2011`), else none.
pub(super) fn year_of(date: &str) -> Option<&str> {
    date.get(..4).filter(|four| four.bytes().all(|b| b.is_ascii_digit()))
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}
