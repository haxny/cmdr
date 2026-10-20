//! Where the restic view lives in a path, and what a path inside it names.
//!
//! Both answers are lexical: no listing, no `stat`. A host gates every path
//! through [`restic_route`] on its hot path, and confirms the candidate repo
//! once, through the hub's memo.

use crate::snapshots::{LATEST_FOLDER_NAME, SHORT_ID_LEN};
use std::ffi::OsString;
use std::path::{Component, Path};

// The lexical naming lives in `cmdr-fs`, so the listing engine and the write
// guards can ask it without linking `rustic_core`; one definition, re-exported.
// A host confirms `restic_route`'s candidate through
// `ResticRepos::confirm_repo` before it routes.
pub use cmdr_fs::restic_view::{VIRTUAL_ENTRY_NAME, restic_route};

/// Which snapshot a folder name inside the view stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnapshotRef {
    /// The `latest` folder.
    Latest,
    /// A dated folder, by the id prefix its name ends with.
    ShortId(String),
}

/// What a path inside the view names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VirtualPath {
    /// The view's own folder: the snapshot list.
    Root,
    /// A snapshot folder, or something inside one (`inner` empty for the folder
    /// itself), one name per component as stored.
    Snapshot(SnapshotRef, Vec<OsString>),
}

/// Classifies `path` against the view's root folder (`<repo>/Browse
/// snapshots`). A relative `path` is read against `view_root`.
///
/// `None` for a path outside the view, one that climbs (`..`), and a first
/// component that names no snapshot. A dated folder resolves by its id suffix
/// alone: the time in front is local time, so a time zone change or DST would
/// otherwise orphan every saved path.
pub(crate) fn classify(view_root: &Path, path: &Path) -> Option<VirtualPath> {
    let rest = if path.is_absolute() {
        path.strip_prefix(view_root).ok()?
    } else {
        path
    };
    let mut names = Vec::new();
    for component in rest.components() {
        match component {
            Component::Normal(name) => names.push(name.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let mut names = names.into_iter();
    let Some(first) = names.next() else {
        return Some(VirtualPath::Root);
    };
    let first = first.to_str()?;
    let snapshot = if first == LATEST_FOLDER_NAME {
        SnapshotRef::Latest
    } else {
        SnapshotRef::ShortId(short_id_of(first)?.to_string())
    };
    Some(VirtualPath::Snapshot(snapshot, names.collect()))
}

/// The id prefix a dated folder name ends with: `… (a1b2c3d4)`. Lowercase hex,
/// [`SHORT_ID_LEN`] to 64 digits, as the names are minted.
pub(crate) fn short_id_of(folder_name: &str) -> Option<&str> {
    let open = folder_name.strip_suffix(')')?.rfind('(')?;
    let hex = &folder_name[open + 1..folder_name.len() - 1];
    let well_formed = (SHORT_ID_LEN..=64).contains(&hex.len())
        && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    well_formed.then_some(hex)
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
