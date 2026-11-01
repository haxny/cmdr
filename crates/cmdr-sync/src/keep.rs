//! Where replaced and removed items go: the Trash, or a dated versions folder.
//! Pure path math; rules in `DETAILS.md` § Keep.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SyncCaps;

/// The default versions folder, relative to the target root. Under the app-owned
/// `.cmdr-sync`, which every compare skips.
pub const DEFAULT_VERSIONS_FOLDER: &str = ".cmdr-sync/versions";
/// What `{profile}` reads when the run has no profile.
const NO_PROFILE: &str = "No profile";

/// Whether kept items go to the Trash or the versions folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeepPlace {
    /// The Trash where the volume has one, else the versions folder.
    TrashElseVersions,
    /// Always the versions folder.
    VersionsAlways,
    /// The Trash only; on a volume without one, those rows are skipped.
    TrashOnly,
}

/// Where the versions folder lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VersionsLocation {
    /// Inside the target root, at this relative path.
    TargetRoot {
        /// A relative path, `/`-separated.
        folder_name: String,
    },
    /// One folder on the local disk for every sync: an absolute path that may
    /// hold `{root}`, `{profile}`, and `{date}`.
    Custom {
        /// The path template.
        template: String,
    },
}

/// How a run keeps what it replaces or removes. Nothing is ever deleted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeepPolicy {
    /// Trash or versions folder.
    pub place: KeepPlace,
    /// Where the versions folder is.
    pub location: VersionsLocation,
}

impl Default for KeepPolicy {
    fn default() -> Self {
        Self {
            place: KeepPlace::TrashElseVersions,
            location: VersionsLocation::TargetRoot {
                folder_name: DEFAULT_VERSIONS_FOLDER.to_owned(),
            },
        }
    }
}

/// When a run started, as its versions folder names it: `YYYY-MM-DD HHMMSS`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunStamp(String);

impl RunStamp {
    /// The stamp of a local date and time.
    pub fn new(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self(format!("{year:04}-{month:02}-{day:02} {hour:02}{minute:02}{second:02}"))
    }

    /// The stamp as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which volume a path is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// The volume of the side being written (a rename away).
    Target,
    /// The local disk (a custom versions folder).
    LocalDisk,
}

/// One run's versions folder on one side.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionsRoot {
    /// Which volume it is on.
    pub place: Place,
    /// The dated folder itself.
    pub dir: PathBuf,
}

/// Why an item can't be kept, so it is left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeepRefusal {
    /// Trash only, and the volume has no Trash.
    NoTrash,
    /// The path would leave its folder (`..`, an absolute or empty part).
    EscapesFolder,
    /// The custom folder isn't an absolute path, or has an unknown or unclosed
    /// placeholder.
    BadTemplate,
}

/// Where one item goes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeepDestination {
    /// The volume's Trash.
    Trash,
    /// Into this folder (its name is kept).
    Folder {
        /// Which volume the folder is on.
        place: Place,
        /// The folder.
        dir: PathBuf,
    },
    /// Nowhere: the item is left alone.
    Refused(KeepRefusal),
}

/// The parts of a `/`-separated relative path, refused if any part could
/// leave the folder.
fn relative_parts(rel: &str) -> Result<Vec<&str>, KeepRefusal> {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(KeepRefusal::EscapesFolder);
    }
    Ok(parts)
}

fn stamp_folder(stamp: &RunStamp, attempt: u32) -> String {
    if attempt == 0 {
        stamp.as_str().to_owned()
    } else {
        format!("{} ({})", stamp.as_str(), u64::from(attempt) + 1)
    }
}

fn climbs(path: &Path) -> bool {
    path.components().any(|part| matches!(part, Component::ParentDir))
}

/// The template with its placeholders filled in, and whether `{date}` was one.
fn expand(template: &str, root: &str, profile: &str, date: &str) -> Result<(String, bool), KeepRefusal> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut dated = false;
    while let Some(open) = rest.find(['{', '}']) {
        let (before, tail) = rest.split_at(open);
        out.push_str(before);
        let close = tail
            .find('}')
            .filter(|_| tail.starts_with('{'))
            .ok_or(KeepRefusal::BadTemplate)?;
        let value = match &tail[1..close] {
            "root" => root,
            "profile" => profile,
            "date" => {
                dated = true;
                date
            }
            _ => return Err(KeepRefusal::BadTemplate),
        };
        out.push_str(value);
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    Ok((out, dated))
}

fn custom_root(template: &str, target_root: &Path, profile: Option<&str>, date: &str) -> Result<PathBuf, KeepRefusal> {
    if !template.starts_with('/') {
        return Err(KeepRefusal::BadTemplate);
    }
    let root = target_root.to_string_lossy();
    let profile = profile.unwrap_or(NO_PROFILE).replace('/', "-");
    let (expanded, dated) = expand(template, root.trim_start_matches('/'), &profile, date)?;
    // Past the leading `/`, every part must be a plain name.
    relative_parts(expanded.trim_end_matches('/').trim_start_matches('/'))?;
    let dir = PathBuf::from(expanded);
    Ok(if dated { dir } else { dir.join(date) })
}

/// This run's versions folder for a target root. `attempt` above zero names a
/// fresh folder when one with this stamp already exists.
pub fn versions_root(
    policy: &KeepPolicy,
    target_root: &Path,
    profile_name: Option<&str>,
    stamp: &RunStamp,
    attempt: u32,
) -> Result<VersionsRoot, KeepRefusal> {
    if climbs(target_root) {
        return Err(KeepRefusal::EscapesFolder);
    }
    let date = stamp_folder(stamp, attempt);
    match &policy.location {
        VersionsLocation::TargetRoot { folder_name } => {
            let parts = relative_parts(folder_name)?;
            let dir = parts.iter().fold(target_root.to_path_buf(), |dir, part| dir.join(part));
            Ok(VersionsRoot {
                place: Place::Target,
                dir: dir.join(date),
            })
        }
        VersionsLocation::Custom { template } => Ok(VersionsRoot {
            place: Place::LocalDisk,
            dir: custom_root(template, target_root, profile_name, &date)?,
        }),
    }
}

/// Where the item at `rel_path` (from the target root, `/`-separated) goes
/// under `policy` on a volume that can do `target`. A folder destination keeps
/// the item's parent folders under the versions folder.
pub fn keep_destination(
    policy: &KeepPolicy,
    target: &SyncCaps,
    versions: &VersionsRoot,
    rel_path: &str,
) -> KeepDestination {
    let parts = match relative_parts(rel_path) {
        Ok(parts) => parts,
        Err(refusal) => return KeepDestination::Refused(refusal),
    };
    let folder = || KeepDestination::Folder {
        place: versions.place,
        dir: parts[..parts.len() - 1]
            .iter()
            .fold(versions.dir.clone(), |dir, part| dir.join(part)),
    };
    match (policy.place, target.has_trash) {
        (KeepPlace::TrashElseVersions | KeepPlace::TrashOnly, true) => KeepDestination::Trash,
        (KeepPlace::TrashElseVersions, false) | (KeepPlace::VersionsAlways, _) => folder(),
        (KeepPlace::TrashOnly, false) => KeepDestination::Refused(KeepRefusal::NoTrash),
    }
}
