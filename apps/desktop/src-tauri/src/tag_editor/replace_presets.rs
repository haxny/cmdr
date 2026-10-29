//! The Tags window's saved F4 Replace lists: named lists of find/replace steps in the user's
//! order, cached in memory and backed by `tag-replace-presets.json` in the app data dir. Built like
//! `crate::multi_rename::preset_store` (one locked step and one durable write per change; a file that
//! won't read is set aside, never saved over), plus built-ins: `SEEDS` is append-only, the file
//! remembers how many it took (`seedVersion`), so a deleted built-in never comes back and a new
//! one reaches existing installs once.
//!
//! Running a list is `TagTextTool::Steps` (one edit, one undo step).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::text_tools::ReplaceStep;
use crate::ignore_poison::IgnorePoison;
use crate::state_recovery::{StateFile, StateRead};

const FILENAME: &str = "tag-replace-presets.json";
const LOG_TARGET: &str = "tag_editor::replace_presets";

/// The file's format. Don't bump: it would wipe the user's lists.
const SCHEMA_VERSION: u32 = 1;

/// How many lists the store holds; saving a new one past it is refused.
pub const MAX_PRESETS: usize = 200;

/// One saved list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreset {
    pub id: String,
    pub name: String,
    pub steps: Vec<ReplaceStep>,
}

/// Why a save didn't change the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ReplacePresetRefusal {
    /// The list holds `max` lists already, and this name is a new one.
    ListFull { max: u32 },
    /// A list needs a name and at least one step.
    Empty,
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Saves `preset` over the list with its name, or at the end as a new one.
pub fn save_in(
    presets: &mut Vec<ReplacePreset>,
    mut preset: ReplacePreset,
    max: usize,
) -> Result<(), ReplacePresetRefusal> {
    let key = name_key(&preset.name);
    if key.is_empty() || preset.steps.is_empty() {
        return Err(ReplacePresetRefusal::Empty);
    }
    preset.name = preset.name.trim().to_string();
    if let Some(at) = presets.iter().position(|p| name_key(&p.name) == key) {
        preset.id = presets[at].id.clone();
        presets[at] = preset;
        return Ok(());
    }
    if presets.len() >= max {
        return Err(ReplacePresetRefusal::ListFull {
            max: u32::try_from(max).unwrap_or(u32::MAX),
        });
    }
    if presets.iter().any(|p| p.id == preset.id) {
        preset.id = uuid::Uuid::new_v4().to_string();
    }
    presets.push(preset);
    Ok(())
}

/// Deletes list `id`. Returns whether it was there.
pub fn delete_in(presets: &mut Vec<ReplacePreset>, id: &str) -> bool {
    let before = presets.len();
    presets.retain(|p| p.id != id);
    presets.len() != before
}

/// The lists, and how many built-ins this file has taken.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetList {
    pub entries: Vec<ReplacePreset>,
    pub seed_version: u32,
}

/// A built-in list. Append new ones at the END of `SEEDS`; never reorder or remove.
struct Seed {
    name: &'static str,
    build: fn() -> Vec<ReplaceStep>,
}

const SEEDS: &[Seed] = &[Seed {
    name: "Czech part words → numbers",
    build: czech_part_steps,
}];

/// Czech ordinals 1..=20, each in its masculine and feminine form, with and without diacritics.
const ORDINALS: [&str; 20] = [
    "první|prvni",
    "druhý|druhá|druhy|druha",
    "třetí|treti",
    "čtvrtý|čtvrtá|ctvrty|ctvrta",
    "pátý|pátá|paty|pata",
    "šestý|šestá|sesty|sesta",
    "sedmý|sedmá|sedmy|sedma",
    "osmý|osmá|osmy|osma",
    "devátý|devátá|devaty|devata",
    "desátý|desátá|desaty|desata",
    "jedenáctý|jedenáctá|jedenacty|jedenacta",
    "dvanáctý|dvanáctá|dvanacty|dvanacta",
    "třináctý|třináctá|trinacty|trinacta",
    "čtrnáctý|čtrnáctá|ctrnacty|ctrnacta",
    "patnáctý|patnáctá|patnacty|patnacta",
    "šestnáctý|šestnáctá|sestnacty|sestnacta",
    "sedmnáctý|sedmnáctá|sedmnacty|sedmnacta",
    "osmnáctý|osmnáctá|osmnacty|osmnacta",
    "devatenáctý|devatenáctá|devatenacty|devatenacta",
    "dvacátý|dvacátá|dvacaty|dvacata",
];

/// `, část první` → ` 01` … `, díl dvacátá` → ` 20` on Title, one step per number (so each
/// replacement is a plain literal). The comma and space before the word go too.
fn czech_part_steps() -> Vec<ReplaceStep> {
    ORDINALS
        .iter()
        .enumerate()
        .map(|(at, forms)| ReplaceStep {
            fields: Some(vec![cmdr_tags::TagField::Title]),
            search: format!(r",? ?(?:část|cast|díl|dil) (?:{forms})(?:\b|$)"),
            replace: format!(" {:02}", at + 1),
            case_sensitive: false,
            first_only: false,
            regex: true,
        })
        .collect()
}

/// Adds the built-ins this list hasn't taken yet (those past its `seed_version`), skipping a name the
/// list already has. Returns whether it changed anything.
pub fn seed_in(list: &mut PresetList, max: usize) -> bool {
    let current = u32::try_from(SEEDS.len()).unwrap_or(u32::MAX);
    if list.seed_version >= current {
        return false;
    }
    for seed in &SEEDS[list.seed_version as usize..] {
        let key = name_key(seed.name);
        if list.entries.len() >= max || list.entries.iter().any(|p| name_key(&p.name) == key) {
            continue;
        }
        list.entries.push(ReplacePreset {
            id: uuid::Uuid::new_v4().to_string(),
            name: seed.name.to_string(),
            steps: (seed.build)(),
        });
    }
    list.seed_version = current;
    true
}

/// On-disk shape. Generic over the entries so a write can serialize a borrowed slice.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<C> {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    /// Absent in a file from before any built-in: takes them all.
    #[serde(default)]
    seed_version: u32,
    #[serde(default)]
    entries: C,
}

/// The saved lists. Loaded at startup; read and written through `crate::commands::tag_editor`.
pub static REPLACE_PRESETS: ReplacePresetStore = ReplacePresetStore::new();

/// The lists, in the user's order. Declared once as [`REPLACE_PRESETS`].
pub struct ReplacePresetStore {
    cached: Mutex<PresetList>,
    /// Serializes the flush from cache to disk, so two commands landing together can't clobber each other.
    disk: Mutex<()>,
}

impl Default for ReplacePresetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplacePresetStore {
    pub const fn new() -> Self {
        Self {
            cached: Mutex::new(PresetList {
                entries: Vec::new(),
                seed_version: 0,
            }),
            disk: Mutex::new(()),
        }
    }

    pub(crate) fn load_at(&self, path: &Path) {
        // The disk guard is released before the cache guard is taken.
        let loaded = {
            let _disk = self.disk.lock_ignore_poison();
            read(path)
        };
        *self.cached.lock_ignore_poison() = loaded;
    }

    /// Runs `change` on the list as ONE locked step, flushes when it says it changed something, and
    /// answers the lists after. A `None` path still changes the list in memory.
    pub(crate) fn edit_at(
        &self,
        path: Option<&Path>,
        op: &str,
        change: impl FnOnce(&mut PresetList) -> bool,
    ) -> Vec<ReplacePreset> {
        let (snapshot, changed) = {
            let mut cached = self.cached.lock_ignore_poison();
            let changed = change(&mut cached);
            (cached.clone(), changed)
        };
        if changed && let Some(path) = path {
            let _disk = self.disk.lock_ignore_poison();
            if let Err(e) = write(path, &snapshot) {
                log::warn!(target: LOG_TARGET, "Couldn't write the replace presets after {op}: {e}");
            }
        }
        snapshot.entries
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
        change: impl FnOnce(&mut PresetList) -> bool,
    ) -> Vec<ReplacePreset> {
        self.edit_at(path(app).as_deref(), op, change)
    }
}

fn path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join(FILENAME))
}

/// Reads the list at `path`. No file is a new install (no built-in taken yet). A file that won't read
/// or parse, or has a schema version we don't know, is set aside and the list starts empty with the
/// built-ins counted as taken: the person's own lists are in the set-aside copy.
fn read(path: &Path) -> PresetList {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        // A killed write's leftover, so it can't shadow a later one.
        let _ = std::fs::remove_file(&tmp);
    }
    let read =
        crate::state_recovery::read_state_file(
            path,
            StateFile::TagReplacePresets,
            |bytes| match serde_json::from_slice::<Envelope<Vec<ReplacePreset>>>(bytes) {
                Ok(envelope) if envelope.schema_version == SCHEMA_VERSION => Ok(envelope),
                Ok(envelope) => Err(format!(
                    "schema version {}, expected {SCHEMA_VERSION}",
                    envelope.schema_version
                )),
                Err(e) => Err(e.to_string()),
            },
        );
    match read {
        StateRead::Read(envelope) => PresetList {
            entries: envelope.entries,
            seed_version: envelope.seed_version,
        },
        StateRead::Missing => PresetList::default(),
        StateRead::SetAside | StateRead::Held => PresetList {
            entries: Vec::new(),
            seed_version: u32::try_from(SEEDS.len()).unwrap_or(u32::MAX),
        },
    }
}

fn write(path: &Path, list: &PresetList) -> Result<(), crate::state_recovery::StateWriteError> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        seed_version: list.seed_version,
        entries: &list.entries,
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::state_recovery::write_state_file(path, &json)
}
