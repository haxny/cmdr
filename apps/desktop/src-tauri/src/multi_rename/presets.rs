//! Saved Multi-Rename settings: named presets, the newest first. TC's F2
//! "Load/save settings" list. The list machinery is `crate::recents`.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};

use super::plan::MultiRenameSpec;

/// How many presets the list keeps.
pub const MAX_PRESETS: usize = 200;

/// The presets list. Loaded at startup; read and written through
/// `crate::commands::multi_rename`.
pub static PRESETS: RecentsFile<MultiRenamePreset> = RecentsFile::new();

/// One saved preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenamePreset {
    pub id: String,
    pub name: String,
    pub spec: MultiRenameSpec,
}

impl RecentEntry for MultiRenamePreset {
    const FILENAME: &'static str = "multi-rename-presets.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename presets";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// A preset is its name: saving under a name that's taken replaces that preset.
    fn dedupe_key(&self) -> String {
        self.name.trim().to_lowercase()
    }
}

/// The settings the sheet last closed with, so the next ⌃M opens where the last
/// one left off (TC keeps them too). One entry, in its own file.
pub static LAST_SPEC: RecentsFile<LastSpec> = RecentsFile::new();

/// The one entry `LAST_SPEC` keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSpec {
    pub id: String,
    pub spec: MultiRenameSpec,
}

impl LastSpec {
    const ID: &'static str = "last";

    pub fn new(spec: MultiRenameSpec) -> Self {
        Self {
            id: Self::ID.to_string(),
            spec,
        }
    }
}

impl RecentEntry for LastSpec {
    const FILENAME: &'static str = "multi-rename-last.json";
    const LOG_TARGET: &'static str = "multi_rename::presets";
    const LOG_NAME: &'static str = "multi-rename last settings";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Always the same entry, so saving replaces it.
    fn dedupe_key(&self) -> String {
        Self::ID.to_string()
    }
}
