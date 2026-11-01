//! Where replaced and removed items go: the Trash, or a dated versions folder.
//! Pure path math; rules in `DETAILS.md` § Keep.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SyncCaps;
use crate::filter::ALWAYS_EXCLUDED;

/// The default versions folder, relative to the target root. Under the app-owned
/// `.cmdr-sync`, which every compare skips.
pub(crate) const DEFAULT_VERSIONS_FOLDER: &str = ".cmdr-sync/versions";
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
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
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
    /// The folder the person chose (the target root, or the custom folder's
    /// part before any placeholder). Everything between it and `dir` is made
    /// or checked by the run, and must not be a symlink.
    pub trusted: PathBuf,
}

/// Why an item can't be kept, so it is left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeepRefusal {
    /// Trash only, and the volume has no Trash.
    NoTrash,
    /// The path would leave its folder (`..`, an absolute or empty part).
    EscapesFolder,
    /// The custom folder isn't an absolute path or has an unknown or unclosed
    /// placeholder, or a target-root folder isn't under `.cmdr-sync/`.
    BadTemplate,
    /// The custom folder lies inside one of the folders being synchronized.
    InsideSyncFolder,
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

/// An absolute path from its non-empty parts, refused on `.` or `..`.
fn absolute_from(path: &str) -> Result<PathBuf, KeepRefusal> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.iter().any(|part| *part == "." || *part == "..") {
        return Err(KeepRefusal::EscapesFolder);
    }
    Ok(parts.iter().fold(PathBuf::from("/"), |dir, part| dir.join(part)))
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

/// The custom folder's part before its first placeholder: the folder the
/// person picked.
fn custom_base(template: &str) -> Result<PathBuf, KeepRefusal> {
    let literal = match template.find('{') {
        Some(open) => template[..open].rfind('/').map_or("/", |slash| &template[..slash]),
        None => template,
    };
    absolute_from(literal)
}

fn custom_root(
    template: &str,
    target_root: &Path,
    profile: Option<&str>,
    date: &str,
) -> Result<VersionsRoot, KeepRefusal> {
    if !template.starts_with('/') {
        return Err(KeepRefusal::BadTemplate);
    }
    let root = target_root.to_string_lossy();
    let profile = profile.unwrap_or(NO_PROFILE).replace('/', "-");
    // An empty expansion (`{root}` of `/`) leaves `//`, which collapses here.
    let (expanded, dated) = expand(template, &root, &profile, date)?;
    let dir = absolute_from(&expanded)?;
    Ok(VersionsRoot {
        place: Place::LocalDisk,
        dir: if dated { dir } else { dir.join(date) },
        trusted: custom_base(template)?,
    })
}

fn target_root_folder(folder_name: &str, target_root: &Path, date: &str) -> Result<VersionsRoot, KeepRefusal> {
    let parts = relative_parts(folder_name)?;
    if parts.len() < 2 || parts[0] != ALWAYS_EXCLUDED {
        return Err(KeepRefusal::BadTemplate);
    }
    let dir = parts.iter().fold(target_root.to_path_buf(), |dir, part| dir.join(part));
    Ok(VersionsRoot {
        place: Place::Target,
        dir: dir.join(date),
        trusted: target_root.to_path_buf(),
    })
}

/// This run's versions folder for a target root. `attempt` above zero names a
/// fresh folder when one with this stamp already exists.
pub(crate) fn versions_root(
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
        VersionsLocation::TargetRoot { folder_name } => target_root_folder(folder_name, target_root, &date),
        VersionsLocation::Custom { template } => custom_root(template, target_root, profile_name, &date),
    }
}

/// Where a run sets a replaced file aside before its copy lands: a versions
/// folder on the target's own volume (a rename, so it can always move back).
/// The policy's own folder when that is in the target root, else the default.
pub(crate) fn staging_root(
    policy: &KeepPolicy,
    target_root: &Path,
    stamp: &RunStamp,
    attempt: u32,
) -> Result<VersionsRoot, KeepRefusal> {
    let folder_name = match &policy.location {
        VersionsLocation::TargetRoot { folder_name } => folder_name.as_str(),
        VersionsLocation::Custom { .. } => DEFAULT_VERSIONS_FOLDER,
    };
    let local = KeepPolicy {
        place: KeepPlace::VersionsAlways,
        location: VersionsLocation::TargetRoot {
            folder_name: folder_name.to_owned(),
        },
    };
    versions_root(&local, target_root, None, stamp, attempt)
}

/// The folder for `rel_path`'s parent under `versions`, or why there is none.
pub(crate) fn kept_dir(versions: &VersionsRoot, rel_path: &str) -> Result<PathBuf, KeepRefusal> {
    let parts = relative_parts(rel_path)?;
    Ok(parts[..parts.len() - 1]
        .iter()
        .fold(versions.dir.clone(), |dir, part| dir.join(part)))
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
    let dir = match kept_dir(versions, rel_path) {
        Ok(dir) => dir,
        Err(refusal) => return KeepDestination::Refused(refusal),
    };
    match (policy.place, target.has_trash) {
        (KeepPlace::TrashElseVersions | KeepPlace::TrashOnly, true) => KeepDestination::Trash,
        (KeepPlace::TrashElseVersions, false) | (KeepPlace::VersionsAlways, _) => KeepDestination::Folder {
            place: versions.place,
            dir,
        },
        (KeepPlace::TrashOnly, false) => KeepDestination::Refused(KeepRefusal::NoTrash),
    }
}
