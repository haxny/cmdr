//! The sync profiles file: `sync-profiles.json` in the app data dir, one list in the
//! person's order plus the options last used per pair of roots. Cached in memory;
//! every change is one locked step and one durable temp+rename write, and a file
//! that won't read is set aside (`crate::state_recovery`), never saved over.
//!
//! Entries stay `serde_json::Value`s so a field a newer build wrote survives this
//! build's save, and a profile this build can't read is listed greyed, not dropped.
//!
//! Lock order: `disk` first, then `cached`, and `disk` is held across a change and its write
//! so writes land in the order the changes were made.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cmdr_sync::{ProfileProblem, RootRef, SyncOptions, SyncProfile, validate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ignore_poison::IgnorePoison;
use crate::state_recovery::{StateFile, StateRead};

const FILENAME: &str = "sync-profiles.json";
const LOG_TARGET: &str = "sync::profiles";

/// The file's format. A newer build's file is set aside rather than read half.
const SCHEMA_VERSION: u32 = 1;

/// How many profiles the list holds; saving a new one past it is refused.
pub const MAX_PROFILES: usize = 200;
/// How many pairs of roots remember their last options.
pub const MAX_REMEMBERED_PAIRS: usize = 50;

/// One row of the profile list: a profile that works, or one shown greyed with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProfileEntry {
    /// Usable.
    Valid {
        /// The profile (boxed: it is far larger than the greyed variant).
        profile: Box<SyncProfile>,
    },
    /// Kept in the file but not usable until the person fixes it.
    Invalid {
        /// Its id.
        id: String,
        /// Its name, as stored.
        name: String,
        /// What is wrong.
        problem: ProfileProblem,
    },
}

/// Why a save didn't change the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProfileSaveRefusal {
    /// The list holds `max` profiles already, and this name is a new one.
    ListFull {
        /// The limit.
        max: u32,
    },
    /// The profile fails validation.
    Invalid {
        /// What is wrong.
        problem: ProfileProblem,
    },
    /// The list changed in memory but couldn't be written to disk.
    WriteFailed,
}

/// Why a rename, delete, move, or remembered-options change didn't fully happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProfileEditRefusal {
    /// Another profile already has that name; nothing changed.
    NameTaken,
    /// The list changed in memory but couldn't be written to disk.
    WriteFailed,
}

/// The change was made in memory but the file couldn't be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteFailed;

/// Which way Move up / Move down takes a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProfileMove {
    Up,
    Down,
}

/// The stored data: profiles in the person's order, and the newest-first options per pair.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProfileList {
    pub(crate) entries: Vec<Value>,
    pub(crate) last_options: Vec<Value>,
}

/// On-disk shape. Generic so a write can serialize borrowed slices.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: Vec<Value>,
    #[serde(default)]
    last_options: Vec<Value>,
}

/// The profiles. Loaded at startup; read and written through `crate::commands::sync`.
pub static PROFILES: ProfileStore = ProfileStore::new();

/// The profiles file's cache. Declared once as [`PROFILES`].
pub struct ProfileStore {
    cached: Mutex<ProfileList>,
    /// Serializes the flush from cache to disk, so two commands landing together
    /// can't clobber each other's write.
    disk: Mutex<()>,
}

impl Default for ProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileStore {
    pub const fn new() -> Self {
        Self {
            cached: Mutex::new(ProfileList {
                entries: Vec::new(),
                last_options: Vec::new(),
            }),
            disk: Mutex::new(()),
        }
    }

    /// The list as the dialog shows it.
    #[cfg(test)]
    pub fn entries(&self) -> Vec<ProfileEntry> {
        entries_of(&self.cached.lock_ignore_poison())
    }

    /// The options last used for this ordered pair of roots.
    pub fn last_options(&self, left: &RootRef, right: &RootRef) -> Option<SyncOptions> {
        last_options_in(&self.cached.lock_ignore_poison(), left, right)
    }

    pub(crate) fn load_at(&self, path: &Path) {
        let _disk = self.disk.lock_ignore_poison();
        let loaded = read(path);
        *self.cached.lock_ignore_poison() = loaded;
    }

    /// Runs `change` on the list as ONE step, flushes it when `change` says it changed
    /// something, and answers the list after, or `WriteFailed` when the flush didn't land
    /// (the change stays in memory). A `None` path (the app data dir didn't resolve) still
    /// changes the list in memory.
    ///
    /// Lock order is always `disk`, then `cached`, and `disk` is held across the change and
    /// the write: two callers can't write out of order, so the file never loses the newer one.
    pub(crate) fn edit_at(
        &self,
        path: Option<&Path>,
        op: &str,
        change: impl FnOnce(&mut ProfileList) -> bool,
    ) -> Result<Vec<ProfileEntry>, WriteFailed> {
        let _disk = self.disk.lock_ignore_poison();
        let (snapshot, changed) = {
            let mut cached = self.cached.lock_ignore_poison();
            let changed = change(&mut cached);
            (cached.clone(), changed)
        };
        if changed
            && let Some(path) = path
            && let Err(e) = write(path, &snapshot)
        {
            log::warn!(target: LOG_TARGET, "Couldn't write the sync profiles after {op}: {e}");
            return Err(WriteFailed);
        }
        Ok(entries_of(&snapshot))
    }

    /// Loads the file into the cache. Call once at startup.
    pub fn load<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        if let Some(path) = path(app) {
            self.load_at(&path);
        }
    }

    /// [`Self::edit_at`] on the app data dir's file.
    pub fn edit<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        op: &str,
        change: impl FnOnce(&mut ProfileList) -> bool,
    ) -> Result<Vec<ProfileEntry>, WriteFailed> {
        self.edit_at(path(app).as_deref(), op, change)
    }

    /// The list as the dialog shows it, without changing anything.
    pub fn list(&self) -> Vec<ProfileEntry> {
        entries_of(&self.cached.lock_ignore_poison())
    }
}

fn path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join(FILENAME))
}

/// Reads the list at `path`. No file is a new install. A file that won't read or parse,
/// or has a schema version we don't know, is set aside and the list starts empty.
fn read(path: &Path) -> ProfileList {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        // A killed write's leftover, so it can't shadow a later one.
        let _ = std::fs::remove_file(&tmp);
    }
    let read = crate::state_recovery::read_state_file(path, StateFile::SyncProfiles, |bytes| {
        match serde_json::from_slice::<Envelope>(bytes) {
            Ok(envelope) if envelope.schema_version == SCHEMA_VERSION => Ok(envelope),
            Ok(envelope) => Err(format!(
                "schema version {}, expected {SCHEMA_VERSION}",
                envelope.schema_version
            )),
            Err(e) => Err(e.to_string()),
        }
    });
    match read {
        StateRead::Read(envelope) => ProfileList {
            entries: envelope.entries,
            last_options: envelope.last_options,
        },
        StateRead::Missing | StateRead::SetAside | StateRead::Held => ProfileList::default(),
    }
}

fn write(path: &Path, list: &ProfileList) -> Result<(), crate::state_recovery::StateWriteError> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        entries: list.entries.clone(),
        last_options: list.last_options.clone(),
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::state_recovery::write_state_file(path, &json)
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn text<'a>(entry: &'a Value, field: &str) -> Option<&'a str> {
    entry.get(field)?.as_str()
}

/// The key an entry is addressed by: its id, or, for an entry that has none, a key built from
/// its place, so it can still be listed greyed and deleted.
fn entry_id(index: usize, stored: &Value) -> String {
    text(stored, "id").map_or_else(|| format!("no-id-{index}"), str::to_owned)
}

fn position_of(list: &ProfileList, id: &str) -> Option<usize> {
    list.entries
        .iter()
        .enumerate()
        .position(|(index, stored)| entry_id(index, stored) == id)
}

/// The list as shown: each stored entry read and validated.
pub(crate) fn entries_of(list: &ProfileList) -> Vec<ProfileEntry> {
    list.entries
        .iter()
        .enumerate()
        .map(|(index, stored)| {
            let id = entry_id(index, stored);
            let name = text(stored, "name").unwrap_or_default().to_owned();
            match serde_json::from_value::<SyncProfile>(stored.clone()) {
                Ok(profile) => match validate(&profile) {
                    Ok(()) => ProfileEntry::Valid {
                        profile: Box::new(profile),
                    },
                    Err(problem) => ProfileEntry::Invalid { id, name, problem },
                },
                Err(_) => ProfileEntry::Invalid {
                    id,
                    name,
                    problem: ProfileProblem::Unreadable,
                },
            }
        })
        .collect()
}

/// Puts `new`'s fields over `stored` field by field, so fields only a newer build knows
/// survive. Objects merge; anything else is replaced.
fn merge(stored: &mut Value, new: Value) {
    match (stored, new) {
        (Value::Object(stored), Value::Object(new)) => {
            for (key, value) in new {
                match stored.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        stored.insert(key, value);
                    }
                }
            }
        }
        (stored, new) => *stored = new,
    }
}

/// `new` over `stored`, keeping unknown fields. A merge that doesn't read back as
/// exactly `profile` (a tagged value that changed variant keeps its old variant's
/// key) is replaced by `new` whole: correct beats lossless.
fn merged(stored: &Value, profile: &SyncProfile, new: Value) -> Value {
    let mut candidate = stored.clone();
    merge(&mut candidate, new.clone());
    match serde_json::from_value::<SyncProfile>(candidate.clone()) {
        Ok(read) if &read == profile => candidate,
        _ => new,
    }
}

/// Saves `profile`: over the stored entry with its id or, failing that, its name, where
/// that one stands; else at the end, so no other profile's number moves. Another entry
/// with the same name is replaced. Refused when it fails validation, or the list is full.
pub(crate) fn save_in(list: &mut ProfileList, profile: SyncProfile) -> Result<(), ProfileSaveRefusal> {
    validate(&profile).map_err(|problem| ProfileSaveRefusal::Invalid { problem })?;
    let key = name_key(&profile.name);
    let by_id = list
        .entries
        .iter()
        .position(|e| text(e, "id") == Some(profile.id.as_str()));
    let by_name = list
        .entries
        .iter()
        .position(|e| text(e, "name").is_some_and(|n| name_key(n) == key));
    let at = match by_id.or(by_name) {
        Some(at) => at,
        None if list.entries.len() >= MAX_PROFILES => {
            return Err(ProfileSaveRefusal::ListFull {
                max: u32::try_from(MAX_PROFILES).unwrap_or(u32::MAX),
            });
        }
        None => {
            list.entries.push(Value::Object(Map::new()));
            list.entries.len() - 1
        }
    };
    let new = serde_json::to_value(&profile).map_err(|_| ProfileSaveRefusal::Invalid {
        problem: ProfileProblem::Unreadable,
    })?;
    let merged = merged(&list.entries[at], &profile, new);
    list.entries[at] = merged;
    // A list that grew two entries with one name heals here.
    let mut index = 0;
    list.entries.retain(|e| {
        let this = index;
        index += 1;
        this == at || text(e, "name").is_none_or(|n| name_key(n) != key)
    });
    Ok(())
}

/// Deletes profile `id`. Returns whether anything went.
pub(crate) fn delete_in(list: &mut ProfileList, id: &str) -> bool {
    let Some(at) = position_of(list, id) else {
        return false;
    };
    list.entries.remove(at);
    true
}

/// A rename that can't happen as asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NameTaken;

/// Renames profile `id` where it stands. Another profile with that name refuses the
/// rename (`NameTaken`) and nothing changes; saving is what replaces. No-op for an unknown
/// id or an empty name. Returns whether anything changed.
pub(crate) fn rename_in(list: &mut ProfileList, id: &str, name: &str) -> Result<bool, NameTaken> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(false);
    }
    let Some(at) = position_of(list, id) else {
        return Ok(false);
    };
    let key = name_key(name);
    let taken = list
        .entries
        .iter()
        .enumerate()
        .any(|(index, e)| index != at && text(e, "name").is_some_and(|n| name_key(n) == key));
    if taken {
        return Err(NameTaken);
    }
    let Some(object) = list.entries[at].as_object_mut() else {
        return Ok(false);
    };
    object.insert("name".to_owned(), Value::String(name.to_owned()));
    Ok(true)
}

/// Swaps profile `id` with its neighbor. Returns whether it moved.
pub(crate) fn move_in(list: &mut ProfileList, id: &str, direction: ProfileMove) -> bool {
    let Some(at) = position_of(list, id) else {
        return false;
    };
    let to = match direction {
        ProfileMove::Up => at.checked_sub(1),
        ProfileMove::Down => Some(at + 1).filter(|&to| to < list.entries.len()),
    };
    let Some(to) = to else {
        return false;
    };
    list.entries.swap(at, to);
    true
}

fn same_pair(stored: &Value, left: &RootRef, right: &RootRef) -> bool {
    let root_is = |field: &str, root: &RootRef| {
        serde_json::from_value::<RootRef>(stored.get(field).cloned().unwrap_or(Value::Null)).is_ok_and(|r| &r == root)
    };
    root_is("left", left) && root_is("right", right)
}

/// The options remembered for the ordered pair, if any.
pub(crate) fn last_options_in(list: &ProfileList, left: &RootRef, right: &RootRef) -> Option<SyncOptions> {
    let stored = list.last_options.iter().find(|e| same_pair(e, left, right))?;
    serde_json::from_value(stored.get("options")?.clone()).ok()
}

/// Remembers `options` for the pair as the newest, forgetting the oldest pairs past
/// `MAX_REMEMBERED_PAIRS`. Returns whether anything changed.
pub(crate) fn remember_options_in(
    list: &mut ProfileList,
    left: &RootRef,
    right: &RootRef,
    options: &SyncOptions,
) -> bool {
    let (Ok(left_json), Ok(right_json), Ok(options_json)) = (
        serde_json::to_value(left),
        serde_json::to_value(right),
        serde_json::to_value(options),
    ) else {
        return false;
    };
    list.last_options.retain(|e| !same_pair(e, left, right));
    list.last_options.insert(
        0,
        serde_json::json!({ "left": left_json, "right": right_json, "options": options_json }),
    );
    list.last_options.truncate(MAX_REMEMBERED_PAIRS);
    true
}
