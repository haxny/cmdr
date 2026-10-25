//! The tag editor's vocabulary: the fields it manages, what a file's tags look
//! like in memory, and how a read or write fails.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A text field the editor manages. The order is the editor's display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TagField {
    /// Track or book title.
    Title,
    /// Artist; for audiobooks, the author.
    Artist,
    /// Album artist; for audiobooks, the author as well.
    AlbumArtist,
    /// Album; for audiobooks, the book.
    Album,
    /// Performer; for audiobooks, the narrator.
    Performer,
    /// Composer; removed for audiobooks.
    Composer,
    /// Release date or year.
    Date,
    /// Genre.
    Genre,
    /// Publisher.
    Publisher,
    /// Translator.
    Translator,
    /// Track number, plain unless the caller passes a known total.
    Track,
    /// Disc number.
    Disc,
    /// Free-form comment.
    Comment,
    /// Longer description.
    Description,
    /// A web address.
    Www,
}

impl TagField {
    /// Every managed field, in display order.
    pub const ALL: [TagField; 15] = [
        TagField::Title,
        TagField::Artist,
        TagField::AlbumArtist,
        TagField::Album,
        TagField::Performer,
        TagField::Composer,
        TagField::Date,
        TagField::Genre,
        TagField::Publisher,
        TagField::Translator,
        TagField::Track,
        TagField::Disc,
        TagField::Comment,
        TagField::Description,
        TagField::Www,
    ];

    /// Whether the field is written as typed. Every other field is stripped of
    /// diacritics on write; comments and descriptions are prose, and web
    /// addresses are not words.
    pub fn keeps_diacritics(self) -> bool {
        matches!(self, TagField::Comment | TagField::Description | TagField::Www)
    }

    /// Whether the field holds several values (repeated Vorbis keys, one
    /// freeform data entry each) instead of exactly one.
    pub fn is_multi(self) -> bool {
        matches!(
            self,
            TagField::Artist
                | TagField::AlbumArtist
                | TagField::Performer
                | TagField::Composer
                | TagField::Genre
                | TagField::Translator
                | TagField::Www
        )
    }
}

/// A front cover: its MIME type and the encoded image bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cover {
    /// MIME type, like `image/jpeg`.
    pub mime: String,
    /// The encoded image.
    pub data: Vec<u8>,
}

/// One file's tags as the editor sees them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTags {
    /// Managed fields. An absent key is an empty field; a value list is never
    /// empty and never holds an empty string.
    pub fields: BTreeMap<TagField, Vec<String>>,
    /// The front cover, if any.
    pub cover: Option<Cover>,
    /// Text items the editor doesn't manage, by format-native key. Shown, never
    /// written.
    pub extras: BTreeMap<String, Vec<String>>,
}

/// What kind of content a batch of files is, which decides how fields map
/// (audiobooks drop the composer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    /// Spoken-word books.
    Audiobook,
    /// Music.
    Music,
}

/// Why a tag read or write didn't happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagError {
    /// The file's format isn't one the editor handles.
    NotSupported,
    /// A filesystem failure, with its kind and the OS message.
    Io(std::io::ErrorKind, String),
    /// The file's tags couldn't be parsed.
    Parse(String),
    /// The tags couldn't be written.
    Write(String),
}

impl fmt::Display for TagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagError::NotSupported => write!(f, "format not supported"),
            TagError::Io(kind, msg) => write!(f, "I/O error ({kind}): {msg}"),
            TagError::Parse(msg) => write!(f, "couldn't parse tags: {msg}"),
            TagError::Write(msg) => write!(f, "couldn't write tags: {msg}"),
        }
    }
}

impl std::error::Error for TagError {}

impl From<std::io::Error> for TagError {
    fn from(err: std::io::Error) -> Self {
        TagError::Io(err.kind(), err.to_string())
    }
}
