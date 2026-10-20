//! What becomes of an "Open with" copy after its pull (`open_with_extract.rs`).
//!
//! An app can save into its copy: the file is `0o444`, but its folder stays writable,
//! so an atomic save (vim `:w!`, BBEdit's safe save) replaces it, and "Save a copy"
//! lands beside it. That content exists nowhere else, so a copy is only ever removed
//! once it's provably as pulled:
//!
//! - **Each pull leaves a record** ([`record_pull`]): a small sidecar in its launch
//!   folder naming the file and its size and mtime right after the pull.
//! - **A copy is untouched** when its folder holds exactly that file and the record, and
//!   the file's size and mtime still match. Anything else (an edit, a second file, no
//!   record at all) counts as changed.
//! - **The startup reaper** ([`reap_launch_folders`]) removes an untouched copy and moves
//!   everything in a changed one to the Trash, under its own name, logging at `warn`.
//!   When the Trash refuses an item, the folder stays for the next launch: ❌ never a
//!   delete in its place.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The record's name inside a launch folder. Dot-led and Cmdr-prefixed, so no pulled
/// file plausibly carries it; [`record_pull`] refuses one that does.
const RECORD_NAME: &str = ".cmdr-open-with-pull.json";

/// What one pull left in its launch folder.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PullRecord {
    /// The copy's file name inside the folder.
    file_name: String,
    size: u64,
    modified: FileTime,
}

/// A file's mtime to the nanosecond, as seconds and nanoseconds since the Unix epoch.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
struct FileTime {
    secs: u64,
    nanos: u32,
}

impl FileTime {
    fn of(time: SystemTime) -> Option<Self> {
        let since = time.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            secs: since.as_secs(),
            nanos: since.subsec_nanos(),
        })
    }
}

/// Records `copy` as pulled, beside it in its launch folder. Best-effort: a copy with
/// no record counts as changed, so a failure here only means it's kept, never lost.
pub(super) fn record_pull(copy: &Path) {
    let result = (|| -> Result<(), String> {
        let folder = copy.parent().ok_or("no launch folder")?;
        let file_name = copy.file_name().ok_or("no file name")?.to_string_lossy().into_owned();
        if file_name == RECORD_NAME {
            return Err("the pulled file has the record's own name".to_string());
        }
        let meta = std::fs::symlink_metadata(copy).map_err(|e| e.to_string())?;
        let modified = meta.modified().ok().and_then(FileTime::of).ok_or("no mtime")?;
        let record = PullRecord {
            file_name,
            size: meta.len(),
            modified,
        };
        let json = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        std::fs::write(folder.join(RECORD_NAME), json).map_err(|e| e.to_string())
    })();
    if let Err(e) = result {
        log::debug!(target: "cmdr_lib::file_viewer", "no pull record for {}, so it'll be kept: {e}", copy.display());
    }
}

/// Whether the launch folder at `folder` still holds exactly what its pull left.
fn is_untouched(folder: &Path) -> bool {
    let Some(record) = std::fs::read(folder.join(RECORD_NAME))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PullRecord>(&bytes).ok())
    else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    let only_the_pull = entries.into_iter().all(|entry| {
        entry.is_ok_and(|entry| {
            let name = entry.file_name();
            name == RECORD_NAME || name.to_string_lossy() == record.file_name
        })
    });
    if !only_the_pull {
        return false;
    }
    std::fs::symlink_metadata(folder.join(&record.file_name)).is_ok_and(|meta| {
        meta.is_file()
            && meta.len() == record.size
            && meta.modified().ok().and_then(FileTime::of) == Some(record.modified)
    })
}

/// The startup sweep of the open-with dir: every subdir of `dir` that `is_launch_folder`
/// names goes, an untouched one removed and a changed one's contents handed to `trash`.
/// `trash` is the real Trash in the app and a stand-in under test.
pub(super) fn reap_launch_folders(
    dir: &Path,
    is_launch_folder: impl Fn(&str) -> bool,
    trash: &dyn Fn(&Path) -> Result<(), String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // dir doesn't exist yet (first run) — nothing to reap.
    };
    for entry in entries.flatten() {
        if !is_launch_folder(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let folder = entry.path();
        if !is_untouched(&folder) && !trash_contents(&folder, trash) {
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(&folder) {
            log::debug!(target: "cmdr_lib::file_viewer", "couldn't remove open-with copy {}: {e}", folder.display());
        }
    }
}

/// Moves everything in `folder` but its record to the Trash. Answers whether the folder
/// is now safe to remove, which it isn't while anything the Trash refused is still in it.
fn trash_contents(folder: &Path, trash: &dyn Fn(&Path) -> Result<(), String>) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    let kept: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name() != RECORD_NAME)
        .map(|entry| entry.path())
        .collect();
    let mut all_trashed = true;
    for item in kept {
        match trash(&item) {
            Ok(()) => log::warn!(
                target: "cmdr_lib::file_viewer",
                "an app changed an open-with copy, so it went to the Trash instead of being removed: {}",
                item.display()
            ),
            Err(e) => {
                all_trashed = false;
                log::warn!(
                    target: "cmdr_lib::file_viewer",
                    "an app changed an open-with copy and the Trash refused it, so it stays for the next launch: {}: {e}",
                    item.display()
                );
            }
        }
    }
    all_trashed
}
