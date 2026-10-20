//! Reading and writing a backend's saved-server list.
//!
//! ❗ **A saved-server list is a convenience, ❌ never access to anything.** No
//! secret lives in one (those go to the Keychain, keyed by the backend's own
//! service string). But it holds what a person built up over time (accounts,
//! bucket places, pins, names, auto-reconnect switches), so a file that's there
//! but doesn't read is never saved over: `crate::state_recovery::read_state_file`
//! sets it aside first, and a list whose entries are `lenient_entries` keeps every
//! entry that reads and a copy of the whole file. A startup still never refuses to
//! run over a broken list.
//!
//! Each backend keeps its own in-memory mirror and its own `OnceLock<PathBuf>`;
//! what lives here is only the file handling they have in common.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::state_recovery::StateFile;

/// Reads `file_name` out of the app data dir into `T`, and hands back the path
/// to write it to.
///
/// `None` only when the app data dir can't be resolved, which means there is
/// nowhere to persist to at all. See [`read`] for the rest.
pub fn load<R: tauri::Runtime, T: Default + DeserializeOwned>(
    app: &tauri::AppHandle<R>,
    file_name: &str,
    file: StateFile,
) -> Option<(T, PathBuf)> {
    let dir = crate::config::resolved_app_data_dir(app).ok()?;
    let path = dir.join(file_name);
    let store = read(&path, file);
    Some((store, path))
}

/// Reads the list at `path` into `T`. A missing file is `T::default()`; so is one
/// that can't be read or parsed, which is set aside first (`file` names it in the
/// notice). When the set-aside can't happen, [`save`] refuses to write over it for
/// the session.
///
/// ❗ A `.tmp` left by a crash mid-write is swept first: `write_state_file`
/// writes there and renames, so one still sitting beside the file is a write
/// that never landed and would otherwise be picked up by nothing and cleaned by
/// no one.
pub fn read<T: Default + DeserializeOwned>(path: &Path, file: StateFile) -> T {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
    crate::state_recovery::read_state_file(path, file, |bytes| {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    })
    .into_value()
    .unwrap_or_default()
}

/// Writes `store` back to `path`, durably (temp + rename).
///
/// `list` names the list in the warning a failure logs, so two backends' lines
/// can't be read as one another's.
pub fn save<T: Serialize>(path: &Path, list: &str, store: &T) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        return;
    };
    match crate::state_recovery::write_state_file(path, &json) {
        Ok(()) => {}
        Err(crate::state_recovery::StateWriteError::NotLoaded) => {
            log::warn!(target: "volume", "didn't write server list: list_kind={list}, reason=not_loaded");
        }
        Err(crate::state_recovery::StateWriteError::Io(e)) => {
            log::warn!(target: "volume", "couldn't write server list: source=os, error_kind={:?}, code={:?}, list_kind={list}, detail={:?}", e.kind(), e.raw_os_error(), cmdr_fs::log_detail::LogDetail(&e.to_string()));
        }
    }
}

#[cfg(test)]
#[path = "server_list_file_test.rs"]
mod server_list_file_test;
