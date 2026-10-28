//! The app's one undo store (`<app data>/tag-undo`), and undoing a tag write.
//!
//! The store has one writer at a time, so it lives behind a `Mutex` held for a whole apply or
//! undo. Lock order: the store is outermost; under it, Multi-Rename's sessions and the tag
//! sessions are taken one after another, never nested (the only nesting is rename → tag, in
//! Multi-Rename's close and evict). ❌ Never take the store while holding either of the others.

use std::sync::Mutex;

use cmdr_tags::{ApplyId, RestoreReport, UndoLimits, UndoStore, WriteOptions};

use super::apply::{panicked, reread_media};
use super::session::{LOCAL_VOLUME_ID, TagEditorError, reload};
use crate::crash_reporter::contain_panics;
use crate::ignore_poison::IgnorePoison;

const LOG_TARGET: &str = "tag_editor::undo";

/// Opened on first use.
static STORE: Mutex<Option<UndoStore>> = Mutex::new(None);

/// Runs `f` with the app's undo store, opening it on first use, and holding it throughout.
pub(super) fn with_store<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    f: impl FnOnce(&UndoStore) -> Result<T, TagEditorError>,
) -> Result<T, TagEditorError> {
    let mut slot = STORE.lock_ignore_poison();
    let store = match &mut *slot {
        Some(store) => store,
        empty => empty.insert(open_store(app)?),
    };
    f(store)
}

/// The store at `<app data>/tag-undo`.
fn open_store<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<UndoStore, TagEditorError> {
    let data_dir = crate::config::resolved_app_data_dir(app).map_err(|_| {
        // The reason names the folder, so it isn't logged.
        log::warn!(target: LOG_TARGET, "The app data folder for tag undo isn't available");
        TagEditorError::NoAppDataDir
    })?;
    UndoStore::open(data_dir.join("tag-undo"), UndoLimits::default()).map_err(|error| TagEditorError::Undo { error })
}

/// Writes the old tags of apply `apply_id` back, for the sheet whose tag session is `undoing`.
/// The files it restored are read again into every open tag session that holds them (another
/// sheet's keeps its unwritten edits; see [`reload`]), and Multi-Rename's media cache reads them
/// again. A file changed since the write (a later write, or another file renamed onto its path)
/// is left alone and reported as `changed`. A restore without failures forgets the record; one
/// with failures keeps it, so the user can try again.
pub fn undo<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    apply_id: String,
    undoing: Option<String>,
) -> Result<RestoreReport, TagEditorError> {
    with_store(app, |store| {
        let report = undo_with(store, &apply_id, undoing.as_deref())?;
        // The folder's last write loses this apply, or keeps it with the files still to restore.
        super::last_apply::undone(&apply_id, report.failed.len());
        Ok(report)
    })
}

/// [`undo`] against `store`. The caller holds the store for the whole call.
pub(super) fn undo_with(
    store: &UndoStore,
    apply_id: &str,
    undoing: Option<&str>,
) -> Result<RestoreReport, TagEditorError> {
    let id = ApplyId(apply_id.to_owned());
    // Restoring parses and writes each file with lofty.
    let report = contain_panics(|| store.restore(&id, &WriteOptions::default()))
        .unwrap_or_else(|| Err(panicked()))
        .map_err(|error| TagEditorError::Undo { error })?;
    if report.failed.is_empty()
        && let Err(error) = store.forget(&id)
    {
        log::warn!(target: LOG_TARGET, "Couldn't forget an undone tag write: {error:?}");
    }
    reload(&report.restored, undoing);
    reread_media(LOCAL_VOLUME_ID, &report.restored);
    Ok(report)
}
