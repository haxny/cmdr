//! Why a tag-editor call didn't answer: one typed vocabulary for every tag-editor command, worded by the UI from
//! the variant, never from a message.

use serde::Serialize;

use cmdr_tags::{PatternError, TagError};

use crate::multi_rename::mask::MaskError;

/// Why a tag-editor call didn't answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TagEditorError {
    /// The Multi-Rename session is gone.
    NoSession,
    /// `open` wasn't called for this session.
    NotOpen,
    /// The session's volume is read-only for the tag editor.
    ReadOnly,
    /// An edit names a path that isn't a readable track of the session. Nothing was applied.
    UnknownTrack,
    /// The undo store refused: recording before a write (nothing was written), or an undo.
    Undo { error: TagError },
    /// The app's data folder, which holds the undo store, isn't available. Nothing was written.
    NoAppDataDir,
    /// A cover file name that isn't a plain file name inside the session's folder.
    BadCoverName,
    /// The cover file is over the size limit (10 MiB).
    CoverTooLarge,
    /// The cover file couldn't be read (or isn't a regular file, or has an unsupported name).
    CoverUnreadable,
    /// The cover file's bytes aren't a JPEG, PNG, or WebP image, whatever its name says.
    CoverNotAnImage,
    /// Pasted bytes aren't a JPEG, PNG, WebP, or TIFF image (a TIFF is converted). Nothing was changed.
    PastedNotAnImage,
    /// Reading a dropped picture stalled (its drive stopped answering). Nothing was changed, and the read
    /// finishing later changes nothing either.
    CoverTimedOut,
    /// Earlier drops' reads are still stuck, so this one wasn't started. Nothing was changed.
    CoverBusy,
    /// Converting a pasted picture took too long. Nothing was changed, then or later.
    PasteTimedOut,
    /// A dropped cover is a folder, a FIFO, a symlink, or another non-regular file.
    CoverNotAFile,
    /// No readable file carries the cover the key names (any more). Nothing was changed.
    UnknownCover,
    /// The background task running the call failed.
    TaskFailed,
    /// A file-name pattern that can't be used. Nothing was changed.
    BadPattern { error: PatternError },
    /// A find/replace search that isn't a valid regular expression. Nothing was changed.
    BadRegex,
    /// A Format mask that doesn't parse. Nothing was changed.
    BadMask { error: MaskError },
    /// Neither foobar2000 nor the files' default app could be launched.
    PlayerUnavailable,
}
