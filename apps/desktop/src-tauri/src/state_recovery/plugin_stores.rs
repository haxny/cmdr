//! The `tauri-plugin-store` files, checked before anything loads them. The plugin reads a file it
//! can't parse as an empty store and its next save overwrites the evidence, so `setup()` sets an
//! unreadable one aside first and the plugin then starts from no file at all.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Runtime};

use super::{LOG_TARGET, StateFile};

/// The consent settings that default to on. A `settings.json` Cmdr had to set aside may have held
/// a "no" to either, so the fresh file says "no" until the person says otherwise
/// (`lib/settings/definitions/updates-privacy.ts` holds both defaults). `operationLog.enabled`
/// isn't here: undo history never leaves the Mac (David, 2026-10-16).
const CONSENT_OFF_AFTER_RECOVERY: [&str; 2] = ["analytics.enabled", "updates.crashReports"];

/// The four plugin stores, at the paths their readers use. Call from `setup()` after the instance
/// lock and the security log, and before anything reads one of them.
pub fn recover_plugin_stores<R: Runtime>(app: &AppHandle<R>) {
    for (name, file) in [
        ("settings.json", StateFile::Settings),
        ("shortcuts.json", StateFile::Shortcuts),
        ("app-status.json", StateFile::AppStatus),
    ] {
        if let Some(path) = frontend_store_path(app, name) {
            // allowed-discarded-outcome: setup has no surface to report a recovery on; `recover` logs what it did
            recover(&path, file);
        }
    }
    // Rust loads this one by bare name, so it always sits in the plugin's own app data dir.
    match tauri_plugin_store::resolve_store_path(app, "license.json") {
        Ok(path) => {
            // allowed-discarded-outcome: setup has no surface to report a recovery on; `recover` logs what it did
            recover(&path, StateFile::License);
        }
        Err(e) => log::warn!(target: LOG_TARGET, "Couldn't resolve license.json to check it: {e}"),
    }
}

/// Where the frontend's store for `name` lives: `resolveStorePath` (`lib/settings/store-path.ts`)
/// puts it in `CMDR_DATA_DIR` when that's set, and the plugin resolves a bare name otherwise.
fn frontend_store_path<R: Runtime>(app: &AppHandle<R>, name: &str) -> Option<PathBuf> {
    if std::env::var("CMDR_DATA_DIR").is_ok_and(|dir| !dir.is_empty()) {
        return crate::config::resolved_app_data_dir(app).ok().map(|dir| dir.join(name));
    }
    tauri_plugin_store::resolve_store_path(app, name).ok()
}

/// Checks one store file: sweeps temp files a killed save left, and sets the file aside when it
/// isn't the JSON object the plugin reads. A recovered `settings.json` then gets its fresh file.
/// Returns whether it recovered.
fn recover(path: &Path, file: StateFile) -> bool {
    sweep_killed_saves(path);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        Err(e) => {
            // Present but unreadable right now: we can't tell it's damaged, so it stays put.
            log::warn!(target: LOG_TARGET, "Couldn't read {path:?} to check it, leaving it as is: {e}");
            return false;
        }
    };
    // The plugin's own reading: a JSON object. A single wrong value inside one is the store's
    // to drop, not a recovery.
    if serde_json::from_slice::<HashMap<String, serde_json::Value>>(&bytes).is_ok() {
        return false;
    }
    if super::set_aside(path, file).is_none() {
        // The file stays, and the plugin reads it as empty. Nothing better is left to do.
        return true;
    }
    if file == StateFile::Settings {
        write_fresh_settings(path);
    }
    true
}

/// Starts the new `settings.json` with every consent setting that defaults to on set to off.
fn write_fresh_settings(path: &Path) {
    let fresh: serde_json::Map<String, serde_json::Value> = CONSENT_OFF_AFTER_RECOVERY
        .iter()
        .map(|key| ((*key).to_string(), serde_json::Value::Bool(false)))
        .collect();
    let written = serde_json::to_string_pretty(&fresh)
        .map_err(std::io::Error::other)
        .and_then(|json| crate::config::durable_write_json(path, &temp_path(path, "recovery"), &json));
    if let Err(e) = written {
        log::warn!(
            target: LOG_TARGET,
            "Couldn't write the fresh settings file, so this launch reads the defaults: {e}"
        );
    }
}

/// A temp file beside `path` in the patched plugin's naming (`.<name>.<pid>.<n>.tmp`), so the
/// next launch's sweep finds it too.
fn temp_path(path: &Path, tag: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{tag}.tmp"))
}

/// Removes the temp files a save killed before its rename left beside `path`. Only at startup,
/// before anything can be saving.
fn sweep_killed_saves(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!(".{}.", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let entry_name = entry.file_name();
        let entry_name = entry_name.to_string_lossy();
        if entry_name.starts_with(&prefix)
            && entry_name.ends_with(".tmp")
            && let Err(e) = fs::remove_file(entry.path())
        {
            log::warn!(target: LOG_TARGET, "Couldn't remove the leftover temp file {entry_name}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security_log::{self, SecurityEvent};
    use crate::state_recovery::testing;
    use crate::test_support::TestDir;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read the dir")
            .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort_unstable();
        names
    }

    fn copy_in(dir: &Path, name: &str) -> PathBuf {
        let prefix = format!("{name}.broken-");
        let copies: Vec<String> = names_in(dir).into_iter().filter(|n| n.starts_with(&prefix)).collect();
        assert_eq!(copies.len(), 1, "one copy of {name}: {copies:?}");
        dir.join(&copies[0])
    }

    #[test]
    fn a_truncated_settings_file_is_kept_and_the_fresh_one_says_no_to_both_consents() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_settings");
        let path = dir.join("settings.json");
        fs::write(&path, b"{\n  \"analytics.enabled\": fal").expect("write");

        assert!(recover(&path, StateFile::Settings));

        let copy = copy_in(&dir, "settings.json");
        assert_eq!(
            fs::read(&copy).expect("read the copy"),
            b"{\n  \"analytics.enabled\": fal"
        );
        let fresh: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("a fresh file")).expect("valid JSON");
        assert_eq!(
            fresh,
            serde_json::json!({ "analytics.enabled": false, "updates.crashReports": false })
        );
        let loaded = crate::settings::loader::load_settings_from_dir(&dir);
        assert_eq!(
            loaded.crash_reports_enabled,
            Some(false),
            "Rust reads the same launch as off"
        );
        assert_eq!(notices.recoveries().len(), 1);
        assert_eq!(notices.recoveries()[0].file, StateFile::Settings);
    }

    /// One notice and one security event per recovery; the next launch reads the fresh file and
    /// says nothing.
    #[test]
    fn a_recovery_speaks_once_and_a_healthy_file_never() {
        let events = security_log::testing::capture_events();
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_once");
        let path = dir.join("settings.json");
        fs::write(&path, b"").expect("an empty file, as a crash mid-write leaves it");

        assert!(recover(&path, StateFile::Settings));
        assert!(
            !recover(&path, StateFile::Settings),
            "the next launch finds a healthy file"
        );

        assert_eq!(
            events.events(),
            vec![SecurityEvent::StateFileRecovered {
                file: StateFile::Settings
            }]
        );
        assert_eq!(notices.recoveries().len(), 1);
    }

    #[test]
    fn a_corrupt_shortcuts_or_license_file_is_kept_and_starts_from_no_file() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_others");
        for (name, file) in [
            ("shortcuts.json", StateFile::Shortcuts),
            ("license.json", StateFile::License),
        ] {
            let path = dir.join(name);
            fs::write(&path, b"[1, 2").expect("write");
            assert!(recover(&path, file));
            assert!(!path.exists(), "{name}: the plugin starts from no file");
            assert_eq!(fs::read(copy_in(&dir, name)).expect("read the copy"), b"[1, 2");
        }
        let files: Vec<StateFile> = notices.recoveries().iter().map(|r| r.file).collect();
        assert_eq!(files, vec![StateFile::Shortcuts, StateFile::License]);
    }

    #[test]
    fn a_healthy_or_missing_file_is_left_alone() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_healthy");
        let path = dir.join("settings.json");
        assert!(!recover(&path, StateFile::Settings), "no file is a first launch");
        fs::write(&path, b"{\"listing.showHiddenFiles\": \"not a bool\"}").expect("write");
        assert!(
            !recover(&path, StateFile::Settings),
            "a single invalid value is the store's to drop, not a recovery"
        );
        assert_eq!(names_in(&dir), vec!["settings.json"]);
        assert!(notices.recoveries().is_empty());
    }

    /// A save killed between writing its temp file and the rename leaves the temp file behind.
    #[test]
    fn a_killed_saves_temp_file_is_swept() {
        let dir = TestDir::new("plugin_stores_sweep");
        let path = dir.join("shortcuts.json");
        fs::write(&path, b"{}").expect("write");
        fs::write(dir.join(".shortcuts.json.4242.0.tmp"), b"{\"half").expect("write");
        fs::write(dir.join(".settings.json.4242.0.tmp"), b"another store's").expect("write");

        recover(&path, StateFile::Shortcuts);

        assert_eq!(names_in(&dir), vec![".settings.json.4242.0.tmp", "shortcuts.json"]);
    }
}
