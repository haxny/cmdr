//! The tag editor's vocabulary: the fields it manages, what a file's tags look
//! like in memory, and how a read or write fails.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A text field the editor manages. The order is the editor's display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
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
    /// Composer; for audiobooks, the author of the music.
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

    /// Whether the field may keep diacritics (the user's rule: no diacritics in any written
    /// field but these). Comments and descriptions are prose, and web addresses are not
    /// words. `write_tags` writes every value as given; `tools::strip_diacritics` and the app
    /// apply the rule, skipping these fields.
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
#[cfg_attr(feature = "specta", derive(specta::Type))]
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
    /// Text items the editor doesn't manage, by format-native key. A write keeps the
    /// file's own values for a key both have, removes a key missing here, and adds a key
    /// the file lacks (how an undo brings one back).
    pub extras: BTreeMap<String, Vec<String>>,
}

/// What kind of content a batch of files is, which decides how fields map
/// (audiobooks drop ID3's `TPE3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    /// Spoken-word books.
    Audiobook,
    /// Music.
    Music,
}

/// Why a tag read or write didn't happen. Serialized as `{"type": "<variant>", "data": ...}`
/// (camelCase variant names), an `Io` kind as its `std::io::ErrorKind` name (`"NotFound"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum TagError {
    /// The file's format isn't one the editor handles.
    NotSupported,
    /// A filesystem failure, with its kind and the OS message.
    Io(#[serde(with = "io_kind")] std::io::ErrorKind, String),
    /// The file's tags couldn't be parsed.
    Parse(String),
    /// The tags couldn't be written.
    Write(String),
    /// The write would lose these items (sorted IDs such as `TSOP`, `TXXX:Comment`, `COMM:ces`,
    /// `TITLE#2`): values the reader doesn't show that differ from what it shows (a second value
    /// of a single-value field, a legacy or duplicate frame), or on MP3 frames ID3v2.3 has no
    /// place for. Nothing was written; `WriteOptions::allow_frame_loss` writes anyway.
    WouldDrop(Vec<String>),
    /// `FileTags::extras` asks for an extra (this key) that the file lacks and the write
    /// can't add: an empty key, no values, or a key the format's reader would take for a
    /// managed field. Nothing was written.
    UnwritableExtra(String),
}

impl fmt::Display for TagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagError::NotSupported => write!(f, "format not supported"),
            TagError::Io(kind, msg) => write!(f, "I/O error ({kind}): {msg}"),
            TagError::Parse(msg) => write!(f, "couldn't parse tags: {msg}"),
            TagError::Write(msg) => write!(f, "couldn't write tags: {msg}"),
            TagError::WouldDrop(ids) => write!(f, "the write would drop {}", ids.join(", ")),
            TagError::UnwritableExtra(key) => write!(f, "can't add the extra {key:?}"),
        }
    }
}

impl std::error::Error for TagError {}

impl From<std::io::Error> for TagError {
    fn from(err: std::io::Error) -> Self {
        TagError::Io(err.kind(), err.to_string())
    }
}

/// `std::io::ErrorKind` as its name (`"NotFound"`); a name this build doesn't know reads back
/// as `Other`.
mod io_kind {
    use std::io::ErrorKind;

    use serde::{Deserialize, Deserializer, Serializer};

    /// The kinds that read back as themselves.
    const KINDS: [ErrorKind; 39] = [
        ErrorKind::NotFound,
        ErrorKind::PermissionDenied,
        ErrorKind::ConnectionRefused,
        ErrorKind::ConnectionReset,
        ErrorKind::HostUnreachable,
        ErrorKind::NetworkUnreachable,
        ErrorKind::ConnectionAborted,
        ErrorKind::NotConnected,
        ErrorKind::AddrInUse,
        ErrorKind::AddrNotAvailable,
        ErrorKind::NetworkDown,
        ErrorKind::BrokenPipe,
        ErrorKind::AlreadyExists,
        ErrorKind::WouldBlock,
        ErrorKind::NotADirectory,
        ErrorKind::IsADirectory,
        ErrorKind::DirectoryNotEmpty,
        ErrorKind::ReadOnlyFilesystem,
        ErrorKind::StaleNetworkFileHandle,
        ErrorKind::InvalidInput,
        ErrorKind::InvalidData,
        ErrorKind::TimedOut,
        ErrorKind::WriteZero,
        ErrorKind::StorageFull,
        ErrorKind::NotSeekable,
        ErrorKind::QuotaExceeded,
        ErrorKind::FileTooLarge,
        ErrorKind::ResourceBusy,
        ErrorKind::ExecutableFileBusy,
        ErrorKind::Deadlock,
        ErrorKind::CrossesDevices,
        ErrorKind::TooManyLinks,
        ErrorKind::InvalidFilename,
        ErrorKind::ArgumentListTooLong,
        ErrorKind::Interrupted,
        ErrorKind::Unsupported,
        ErrorKind::UnexpectedEof,
        ErrorKind::OutOfMemory,
        ErrorKind::Other,
    ];

    pub(super) fn serialize<S: Serializer>(kind: &ErrorKind, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{kind:?}"))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ErrorKind, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(KINDS
            .into_iter()
            .find(|kind| format!("{kind:?}") == name)
            .unwrap_or(ErrorKind::Other))
    }
}
