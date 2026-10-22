//! Media fields for a session: what the sheet asks about the files' EXIF and
//! tags, and the progress event while they're read.
//!
//! The per-session cache and the background loader land here; until then every
//! answer is "nothing to read".

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use cmdr_media_meta::MediaField;

use super::error::MultiRenameError;
use super::mask::FieldGap;

/// How far a session's media read got. `total` counts media files only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaProgress {
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// The `multi-rename-media-progress` event: a session's read moved on. At most
/// every 250 ms, plus a final one with `done`. ❗ The struct name kebab-cases to
/// the wire event name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameMediaProgress {
    pub session_id: String,
    pub read: usize,
    pub total: usize,
    pub done: bool,
}

/// What the field picker shows: the read's progress, and each field's example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaOverview {
    pub progress: MediaProgress,
    /// Every field, in `MediaField::ALL` order.
    pub fields: Vec<MediaFieldOverview>,
}

/// One field in the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaFieldOverview {
    pub field: MediaField,
    /// The field in its default format, from the first file in rename order that
    /// has it; `None` when no file read so far does.
    pub example: Option<String>,
    /// How many files read so far have it.
    pub files_with_it: usize,
}

/// What one `[=source.field…]` placeholder renders alone on the token editor's
/// sample rows, and how it fares across the whole session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSamples {
    pub samples: Vec<FieldSample>,
    pub usage: FieldUsage,
}

/// The placeholder on one row: its text, or why it has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldSample {
    /// The file's place in the session (what `PreviewRow::row` names).
    pub row: usize,
    pub text: Option<String>,
    pub gap: Option<FieldGap>,
}

/// How the session's files fare with one placeholder, for "38 files use the
/// modified date".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldUsage {
    /// Files with the field's own value.
    pub with_value: usize,
    /// Files without it that take the `|modified` fallback.
    pub fallback: usize,
    /// Files without it and with no fallback: problem rows.
    pub missing: usize,
    /// Files still being read.
    pub reading: usize,
}

/// How far the session's media read got.
pub(crate) fn progress(session_id: &str) -> Result<MediaProgress, MultiRenameError> {
    let _ = session_id;
    Ok(MediaProgress {
        read: 0,
        total: 0,
        done: true,
    })
}

/// The field picker's overview.
pub(crate) fn overview(session_id: &str) -> Result<MediaOverview, MultiRenameError> {
    Ok(MediaOverview {
        progress: progress(session_id)?,
        fields: MediaField::ALL
            .into_iter()
            .map(|field| MediaFieldOverview {
                field,
                example: None,
                files_with_it: 0,
            })
            .collect(),
    })
}

/// The placeholder `token` (its text between the brackets, `=exif.taken:d`)
/// rendered alone on the files at `rows`, and its usage across the session.
pub(crate) fn field_samples(session_id: &str, token: &str, rows: &[usize]) -> Result<FieldSamples, MultiRenameError> {
    let _ = (session_id, token, rows);
    Err(MultiRenameError::Internal {
        detail: "media field samples aren't built yet".to_string(),
    })
}

/// Moves the files at `rows` to the front of the session's read queue.
pub(crate) fn prioritize(session_id: &str, rows: &[usize]) {
    let _ = (session_id, rows);
}
