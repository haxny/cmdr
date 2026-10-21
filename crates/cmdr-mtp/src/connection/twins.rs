//! Same-named siblings, one row each.
//!
//! PTP lets a folder hold two objects with the same name (a camera merging card
//! folders after a counter wrap, an e-reader's own object database, Android's
//! MTP index gone stale), so a name alone can't say which one a row means. A
//! listing gives each twin's row its own path instead: a PICK segment, `␀` (NUL)
//! plus the object's handle, sits right before the shared name:
//! `/DCIM/␀4242/IMG_1.JPG`. The leaf stays the real name, so a copy names its
//! destination after it and a walk below a twin folder builds relative paths
//! from it; only "the folder holding this" has to step over the pick, which is
//! [`folder_of`].
//!
//! NUL because no real name can hold one: Android's names are Linux names (any
//! byte but `/` and NUL) and PTP's strings are NUL-terminated UCS-2, so a pick
//! can't be mistaken for, or collide with, a folder the device holds.

use mtp_rs::ObjectHandle;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// What starts a pick segment.
const PICK: char = '\0';

/// The row path of `handle`, one of several objects in `folder` named `name`.
pub(super) fn picked(folder: &Path, handle: ObjectHandle, name: &OsStr) -> PathBuf {
    folder.join(format!("{PICK}{}", handle.0)).join(name)
}

/// The handle a pick segment names, or `None` for an ordinary segment.
fn pick_of(segment: &OsStr) -> Option<ObjectHandle> {
    let digits = segment.to_str()?.strip_prefix(PICK)?;
    digits.parse().ok().map(ObjectHandle)
}

/// Whether `path`'s last segment is a pick: a twin's row path minus its name.
fn ends_in_pick(path: &Path) -> bool {
    path.file_name().is_some_and(|segment| pick_of(segment).is_some())
}

/// The folder holding `path`'s object: its parent, stepping over a pick. `None`
/// for a root, like [`Path::parent`].
pub(crate) fn folder_of(path: &Path) -> Option<&Path> {
    let parent = path.parent()?;
    if ends_in_pick(parent) {
        parent.parent()
    } else {
        Some(parent)
    }
}

/// The folder `parent` names when a caller hands over a twin's row path minus
/// its name (`Path::parent` of a row path), else `None`. A change reported "in"
/// such a parent is a change to a twin, in the folder this returns.
pub(crate) fn picked_folder(parent: &Path) -> Option<&Path> {
    if ends_in_pick(parent) { parent.parent() } else { None }
}

/// The handle `path` picks for its own object, when it's a twin's row path.
/// A pick further up names an ancestor, which this doesn't report.
pub(crate) fn pick_in(path: &Path) -> Option<ObjectHandle> {
    path.parent().and_then(Path::file_name).and_then(pick_of)
}

/// `path` without its own pick: the path every one of its twins shares. An
/// ordinary path comes back as it is.
pub(super) fn shared_path(path: &Path) -> PathBuf {
    match (pick_in(path), folder_of(path), path.file_name()) {
        (Some(_), Some(folder), Some(name)) => folder.join(name),
        _ => path.to_path_buf(),
    }
}

/// Gives each twin in one folder's listing its own row path, in the entries
/// and in the path-cache updates alike (the listing builds the two in step, one
/// per object). Every other row keeps its plain path.
pub(super) fn give_twins_their_own_paths(
    folder: &Path,
    entries: &mut [cmdr_fs::entry::FileEntry],
    cache_updates: &mut [(PathBuf, ObjectHandle)],
) {
    debug_assert_eq!(entries.len(), cache_updates.len(), "one cache update per row");
    let mut seen: std::collections::HashMap<&Path, usize> = std::collections::HashMap::new();
    for (path, _) in cache_updates.iter() {
        *seen.entry(path.as_path()).or_default() += 1;
    }
    let shared: std::collections::HashSet<PathBuf> = seen
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(path, _)| path.to_path_buf())
        .collect();
    if shared.is_empty() {
        return;
    }
    for (entry, (path, handle)) in entries.iter_mut().zip(cache_updates.iter_mut()) {
        if !shared.contains(path.as_path()) {
            continue;
        }
        let Some(name) = path.file_name() else { continue };
        debug_assert_eq!(
            entry.inode,
            Some(handle.0),
            "the row and its cache update are one object"
        );
        let own = picked(folder, *handle, name);
        entry.path = own.to_string_lossy().into_owned();
        *path = own;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_twin_row_keeps_its_name_and_its_folder() {
        let row = picked(Path::new("/DCIM"), ObjectHandle(42), OsStr::new("IMG_1.JPG"));

        assert_eq!(row.file_name(), Some(OsStr::new("IMG_1.JPG")));
        assert_eq!(folder_of(&row), Some(Path::new("/DCIM")));
        assert_eq!(pick_in(&row), Some(ObjectHandle(42)));
        assert_eq!(shared_path(&row), PathBuf::from("/DCIM/IMG_1.JPG"));
        assert_eq!(
            picked_folder(row.parent().expect("a row has a parent")),
            Some(Path::new("/DCIM"))
        );
    }

    #[test]
    fn a_row_inside_a_twin_folder_is_an_ordinary_child_of_that_folder() {
        let twin_folder = picked(Path::new("/DCIM"), ObjectHandle(7), OsStr::new("Trip"));
        let child = twin_folder.join("a.jpg");

        assert_eq!(folder_of(&child), Some(twin_folder.as_path()));
        assert_eq!(pick_in(&child), None, "the pick names the folder, not the child");
        assert_eq!(shared_path(&child), child);
        assert_eq!(picked_folder(&twin_folder), None);
    }

    #[test]
    fn an_ordinary_path_has_no_pick() {
        assert_eq!(folder_of(Path::new("/DCIM/IMG_1.JPG")), Some(Path::new("/DCIM")));
        assert_eq!(folder_of(Path::new("/IMG_1.JPG")), Some(Path::new("/")));
        assert_eq!(folder_of(Path::new("/")), None);
        assert_eq!(pick_in(Path::new("/DCIM/IMG_1.JPG")), None);
        assert_eq!(picked_folder(Path::new("/DCIM")), None);
    }

    #[test]
    fn a_relative_row_path_steps_over_its_pick_too() {
        let row = picked(Path::new(""), ObjectHandle(3), OsStr::new("IMG_1.JPG"));
        assert_eq!(folder_of(&row), Some(Path::new("")));
    }
}
