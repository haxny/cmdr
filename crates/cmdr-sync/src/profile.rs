//! A saved Synchronize folders profile and the settings it inherits: pure data,
//! validation, and the one place "inherit or override" is decided. The store that
//! keeps profiles on disk is the app's (`sync/profile_store.rs`).

use serde::{Deserialize, Serialize};

use crate::{
    CompareBy, CompareRules, DEFAULT_EXCLUDES, GuardSettings, KeepPolicy, KeepRefusal, MaskFilter, Mode, RunStamp,
    ShowFilters, Side, SyncMaskError, versions_root,
};

/// The longest time tolerance a setting or profile may ask for, in seconds.
const MAX_TOLERANCE_SECS: u64 = 60;

/// One side of a profile: a volume and a path on it. For a remote volume the id is
/// the saved connection's; never a credential.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RootRef {
    /// The volume's id.
    pub volume_id: String,
    /// The folder, `/`-separated.
    pub path: String,
}

/// What the dialog's options say; remembered per profile and per pair of roots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptions {
    /// How the sides should end up.
    pub mode: Mode,
    /// What decides that two files differ.
    pub compare_by: CompareBy,
    /// Descend into folders.
    pub subfolders: bool,
    /// Only the selected entries of each pane.
    pub only_selected: bool,
    /// Leave modification times out of the decision.
    pub ignore_date: bool,
    /// Keep folders with nothing admitted inside.
    pub empty_folders: bool,
    /// TC sync masks.
    pub masks: String,
    /// The editable exclude list.
    pub excludes: Vec<String>,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            mode: Mode::BothWays,
            compare_by: CompareBy::DateAndSize,
            subfolders: true,
            only_selected: false,
            ignore_date: false,
            empty_folders: false,
            masks: String::new(),
            excludes: DEFAULT_EXCLUDES.iter().map(|mask| (*mask).to_owned()).collect(),
        }
    }
}

/// What a profile sets for itself. `None` follows Settings, so a change there
/// reaches every profile that didn't override it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOverrides {
    /// Where replaced and removed files go.
    pub keep: Option<KeepPolicy>,
    /// Times this close count as equal, in seconds.
    pub time_tolerance_secs: Option<u64>,
    /// Treat an exact one-hour difference as equal.
    pub ignore_hour_shift: Option<bool>,
    /// Seconds added to the right side's time (a remote clock's skew); never inherited.
    pub time_offset_secs: i64,
    /// The removal guards.
    pub guards: Option<GuardSettings>,
}

/// The Settings values a compare and a run start from. The frontend reads them
/// from the settings store and passes them in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDefaults {
    /// Where replaced and removed files go.
    pub keep: KeepPolicy,
    /// Times this close count as equal, in seconds.
    pub time_tolerance_secs: u64,
    /// Treat an exact one-hour difference as equal.
    pub ignore_hour_shift: bool,
    /// The removal guards.
    pub guards: GuardSettings,
}

/// A saved profile: two roots, the dialog's options, and what it overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncProfile {
    /// Stable id.
    pub id: String,
    /// The name the person gave it; unique ignoring case and outer spaces.
    pub name: String,
    /// The left folder.
    pub left: RootRef,
    /// The right folder.
    pub right: RootRef,
    /// The dialog's options.
    pub options: SyncOptions,
    /// The show-filters the profile was saved with.
    pub show_filters: ShowFilters,
    /// Open with `show_filters` instead of the default filters.
    pub restore_show_filters: bool,
    /// What this profile sets for itself.
    pub overrides: ProfileOverrides,
}

/// What a compare and a run actually use: Settings, with the profile's overrides on top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// Where replaced and removed files go.
    pub keep: KeepPolicy,
    /// How to compare a pair.
    pub rules: CompareRules,
    /// The removal guards.
    pub guards: GuardSettings,
}

/// Settings, then the options, then the profile's overrides on top.
pub fn effective(defaults: &SyncDefaults, options: &SyncOptions, overrides: &ProfileOverrides) -> Effective {
    Effective {
        keep: overrides.keep.clone().unwrap_or_else(|| defaults.keep.clone()),
        rules: CompareRules {
            by: options.compare_by,
            ignore_date: options.ignore_date,
            tolerance_secs: overrides.time_tolerance_secs.unwrap_or(defaults.time_tolerance_secs),
            ignore_hour_shift: overrides.ignore_hour_shift.unwrap_or(defaults.ignore_hour_shift),
            right_offset_secs: overrides.time_offset_secs,
        },
        guards: overrides.guards.clone().unwrap_or_else(|| defaults.guards.clone()),
    }
}

/// Why a profile can't be used as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProfileProblem {
    /// The name is empty.
    EmptyName,
    /// The masks or the exclude list don't parse.
    BadMask {
        /// What is wrong and where.
        error: SyncMaskError,
    },
    /// The tolerance override is above the limit.
    ToleranceOutOfRange {
        /// The value asked for.
        secs: u64,
    },
    /// The removal percentage override is outside 1 to 100.
    PercentOutOfRange {
        /// The value asked for.
        percent: u8,
    },
    /// The stored profile doesn't read as a profile (a newer build's value, or a
    /// damaged entry). Kept as it is, shown greyed, never dropped.
    Unreadable,
}

/// Whether a profile is usable. The first problem wins.
pub fn validate(profile: &SyncProfile) -> Result<(), ProfileProblem> {
    if profile.name.trim().is_empty() {
        return Err(ProfileProblem::EmptyName);
    }
    MaskFilter::parse(&profile.options.masks, &profile.options.excludes)
        .map_err(|error| ProfileProblem::BadMask { error })?;
    if let Some(secs) = profile.overrides.time_tolerance_secs
        && secs > MAX_TOLERANCE_SECS
    {
        return Err(ProfileProblem::ToleranceOutOfRange { secs });
    }
    if let Some(guards) = &profile.overrides.guards
        && !(1..=100).contains(&guards.max_remove_percent)
    {
        return Err(ProfileProblem::PercentOutOfRange {
            percent: guards.max_remove_percent,
        });
    }
    Ok(())
}

/// Whether the versions folder a policy names can be used at all, for Settings to
/// show next to the field. What depends on the roots (a custom folder inside a
/// synchronized folder, `InsideSyncFolder`) is only known when a run starts.
pub fn check_keep_policy(policy: &KeepPolicy) -> Result<(), KeepRefusal> {
    let stamp = RunStamp::new(2000, 1, 1, 0, 0, 0);
    versions_root(policy, std::path::Path::new("/target"), None, &stamp, 0).map(|_| ())
}

/// How two roots on one volume overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RootConflict {
    /// Both roots are one folder.
    SameFolder,
    /// One root lies inside the other.
    Nested {
        /// The side that holds the other.
        outer: Side,
    },
}

fn parts(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

/// Whether the roots are one folder or one holds the other. Only roots on the same
/// volume can; compared component by component, so `/a` and `/ab` don't conflict.
pub fn roots_conflict(left: &RootRef, right: &RootRef) -> Option<RootConflict> {
    if left.volume_id != right.volume_id {
        return None;
    }
    let (l, r) = (parts(&left.path), parts(&right.path));
    if l == r {
        Some(RootConflict::SameFolder)
    } else if r.starts_with(&l) {
        Some(RootConflict::Nested { outer: Side::Left })
    } else if l.starts_with(&r) {
        Some(RootConflict::Nested { outer: Side::Right })
    } else {
        None
    }
}
