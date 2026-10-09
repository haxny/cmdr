//! IPC for the Multi-Rename Tool (⌃M). Thin: the work is `crate::multi_rename::run`.

use std::sync::Arc;

use tokio::time::Duration;

use crate::deadline::blocking_typed_result_with_timeout;
use crate::multi_rename::names_file::{NameEdit, NamesFileError, read_back};
use crate::multi_rename::plan::{MultiRenameSpec, PreviewRow};
use crate::multi_rename::presets::{
    FIELD_HISTORY, FieldHistoryEntry, LAST_SPEC, LastSpec, MAX_FIELD_HISTORY, MAX_PRESETS, MultiRenamePreset, PRESETS,
    history_entries,
};
use crate::multi_rename::run::{
    ExpectedRename, MultiRenameError, MultiRenameStarted, apply, preview_rows, write_names,
};

/// The live preview: each row's new name and whether it can take it. `rows` are
/// backend row numbers in rename order; `None` previews every row the pane shows.
/// `edits` are names the user typed in Results, by old name.
#[tauri::command]
#[specta::specta]
pub async fn preview_multi_rename(
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
    edits: Vec<NameEdit>,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    // Off the IPC thread: a big folder is a mask and a regex per row.
    blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || preview_rows(&listing_id, include_hidden, rows.as_deref(), &spec, &edits),
    )
    .await
}

/// Renames the rows the user saw as ready (`expected`, from the preview they
/// started from), as one operation the queue shows and Undo reverses. Refuses
/// with `previewOutOfDate` when the folder changed since that preview.
#[tauri::command]
#[specta::specta]
pub async fn apply_multi_rename(
    app: tauri::AppHandle,
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
    edits: Vec<NameEdit>,
    expected: Vec<ExpectedRename>,
) -> Result<MultiRenameStarted, MultiRenameError> {
    let events = Arc::new(crate::file_system::write_operations::TauriEventSink::new(app.clone()));
    let history = history_entries(&spec);
    let started = apply(events, listing_id, include_hidden, rows, spec, edits, expected).await?;
    // What the fields held when a rename ran: TC's per-field history (⌥⇧↓).
    for entry in history {
        FIELD_HISTORY.add(&app, entry, MAX_FIELD_HISTORY);
    }
    Ok(started)
}

/// The text fields' history, newest first, every field together.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_history() -> Vec<FieldHistoryEntry> {
    FIELD_HISTORY.entries(None)
}

/// Results (⌥⏎): writes the preview as `old<TAB>new` lines to a text file and
/// returns its path, for the user's editor.
#[tauri::command]
#[specta::specta]
pub async fn write_multi_rename_names(
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
    edits: Vec<NameEdit>,
) -> Result<String, MultiRenameError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || {
            write_names(&listing_id, include_hidden, rows.as_deref(), &spec, &edits)
                .map(|path| path.to_string_lossy().into_owned())
        },
    )
    .await
}

/// The names the user typed in the Results file, by old name.
#[tauri::command]
#[specta::specta]
pub async fn read_multi_rename_names() -> Result<Vec<NameEdit>, NamesFileError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(2),
        || NamesFileError::Unreadable {
            detail: "timed out".to_string(),
        },
        |detail| NamesFileError::Unreadable { detail },
        read_back,
    )
    .await
}

/// The settings the sheet last closed with, if any.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_last_spec() -> Option<MultiRenameSpec> {
    LAST_SPEC.entries(Some(1)).into_iter().next().map(|last| last.spec)
}

/// Remembers the settings the sheet closes with, for the next ⌃M.
#[tauri::command]
#[specta::specta]
pub fn save_multi_rename_last_spec(app: tauri::AppHandle, spec: MultiRenameSpec) {
    LAST_SPEC.add(&app, LastSpec::new(spec), 1);
}

/// The saved presets, newest first.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_presets() -> Vec<MultiRenamePreset> {
    PRESETS.entries(None)
}

/// Saves a preset; one with the same name is replaced.
#[tauri::command]
#[specta::specta]
pub fn save_multi_rename_preset(app: tauri::AppHandle, preset: MultiRenamePreset) {
    PRESETS.add(&app, preset, MAX_PRESETS);
}

/// Deletes a preset by id. No-op when it isn't there.
#[tauri::command]
#[specta::specta]
pub fn delete_multi_rename_preset(app: tauri::AppHandle, id: String) {
    PRESETS.remove(&app, &id);
}
