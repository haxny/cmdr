//! The Tags window's side of a session (fork): holding one so eviction passes it by, and making
//! one whose media cache starts empty (no EXIF or tag read), from a pane's selection or as a copy
//! of an open sheet's.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use cmdr_collation::NameCollator;

use super::{
    FileEntry, ListingId, ListingLookupError, MultiRenameError, MultiRenameOpened, SESSIONS, insert_new, with_session,
};
use crate::ignore_poison::IgnorePoison;

/// The most audio files a Tags window takes from the folders it was given.
pub(crate) const TAGS_FOLDER_FILE_CAP: usize = 2000;
/// How many levels of subfolders below a given folder a Tags window looks into for audio files.
pub(crate) const TAGS_FOLDER_DEPTH: usize = 8;

/// [`super::open`] for a Tags window: the same files, but its media cache starts empty, so no EXIF or
/// tag read is ever queued for it (the window reads tags itself, through `tag_editor`). A selected folder
/// is opened into the audio files under it (subfolders included), named by their path below the
/// listing's folder (`Book/Disc 1/01.mp3`); a plain file is taken as it is.
pub(crate) fn open_for_tags(
    listing_id: &str,
    include_hidden: bool,
    selected: Option<&[usize]>,
    expected_sequence: u64,
) -> Result<MultiRenameOpened, MultiRenameError> {
    let listings = crate::listing_host::listings();
    let id = ListingId::new(listing_id);
    let gone = || MultiRenameError::Gone {
        listing_id: listing_id.to_string(),
    };
    let lookup_error = |error: ListingLookupError| match error {
        ListingLookupError::Gone { .. } => gone(),
        ListingLookupError::Changed { .. } => MultiRenameError::SelectionChanged {
            listing_id: listing_id.to_string(),
        },
    };
    let rows: Vec<usize> = match selected {
        Some(selected) => selected.to_vec(),
        // Every row: its count first, then its entries, both under the same sequence guard.
        None => (0..listings
            .selection_names(&id, include_hidden, None, expected_sequence)
            .map_err(lookup_error)?
            .len())
            .collect(),
    };
    let entries = listings
        .selection_entries(&id, include_hidden, &rows, expected_sequence)
        .map_err(lookup_error)?;
    let (volume_id, dir) = listings.location(&id).ok_or_else(gone)?;
    let reads_disk = || {
        volume_id == crate::tag_editor::session::LOCAL_VOLUME_ID
            || crate::tag_editor::session::volume_gives_local_paths(&volume_id)
    };
    let names = names_for_tags(&entries, &dir, include_hidden, reads_disk, &listings.collator())?;
    Ok(insert_new(listing_id.to_string(), volume_id, dir, names.into(), &[]))
}

/// The session names for the selected `entries` in `dir`, in selection order: a plain file by its name,
/// a folder by the audio files under it (sorted by `collator` on their relative names). A file reached
/// twice is named once. `reads_disk` says whether the folder's volume gives `std::fs` its files; it is
/// asked only when a folder is selected.
fn names_for_tags(
    entries: &[FileEntry],
    dir: &Path,
    include_hidden: bool,
    reads_disk: impl FnOnce() -> bool,
    collator: &NameCollator,
) -> Result<Vec<String>, MultiRenameError> {
    if entries.iter().any(|entry| entry.is_directory) && !reads_disk() {
        return Err(MultiRenameError::FoldersNeedLocalDisk);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut names = Vec::with_capacity(entries.len());
    let mut walked = 0usize;
    for entry in entries {
        if !entry.is_directory {
            if seen.insert(entry.name.clone()) {
                names.push(entry.name.clone());
            }
            continue;
        }
        let mut found = audio_files_under(dir, &entry.name, include_hidden)?;
        found.sort_by(|a, b| collator.compare(a, b));
        for name in found {
            if !seen.insert(name.clone()) {
                continue;
            }
            walked += 1;
            if walked > TAGS_FOLDER_FILE_CAP {
                return Err(MultiRenameError::TooManyTracks {
                    limit: u32::try_from(TAGS_FOLDER_FILE_CAP).unwrap_or(u32::MAX),
                });
            }
            names.push(name);
        }
    }
    Ok(names)
}

/// The audio files below `dir/folder` (to [`TAGS_FOLDER_DEPTH`] levels, symlinked folders not followed),
/// named by their path below `dir`. Stops with `TooManyTracks` as soon as one folder alone is over the cap.
fn audio_files_under(dir: &Path, folder: &str, include_hidden: bool) -> Result<Vec<String>, MultiRenameError> {
    let walk = walkdir::WalkDir::new(dir.join(folder))
        .max_depth(TAGS_FOLDER_DEPTH + 1) // the files of the deepest folder sit one level lower
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|entry| {
            include_hidden || entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
        });
    let mut found = Vec::new();
    let mut skipped = 0usize;
    for entry in walk {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let Some(name) = entry.path().strip_prefix(dir).ok().and_then(Path::to_str) else {
            skipped += 1;
            continue;
        };
        if entry.depth() == 0 || cmdr_tags::format_of(name).is_none() || !is_file(&entry) {
            continue;
        }
        found.push(name.to_string());
        if found.len() > TAGS_FOLDER_FILE_CAP {
            return Err(MultiRenameError::TooManyTracks {
                limit: u32::try_from(TAGS_FOLDER_FILE_CAP).unwrap_or(u32::MAX),
            });
        }
    }
    if skipped > 0 {
        log::debug!(target: "multi_rename::session_tags", "Tags window: entries under a selected folder that couldn't be read or named: {skipped}");
    }
    Ok(found)
}

/// A file, or a symlink to one (a symlinked folder is never followed, but a linked track is a track).
fn is_file(entry: &walkdir::DirEntry) -> bool {
    let kind = entry.file_type();
    kind.is_file() || (kind.is_symlink() && std::fs::metadata(entry.path()).is_ok_and(|meta| meta.is_file()))
}

/// Holds a session a Tags window owns: `evict_to` passes it by (idle or not) until [`release`].
/// `SessionClosed` when it's gone already.
pub(crate) fn hold(session_id: &str) -> Result<(), MultiRenameError> {
    with_session(session_id, |s| s.held = true)
}

/// Another held session over the same volume, folder, and files (in any order) as `session_id`:
/// the session of a Tags window already showing them. `None` when there's none or it's gone.
pub(crate) fn held_twin(session_id: &str) -> Option<String> {
    let sessions = SESSIONS.lock_ignore_poison();
    let this = sessions.get(session_id)?;
    let sorted = |names: &[String]| {
        let mut names = names.to_vec();
        names.sort_unstable();
        names
    };
    let files = sorted(&this.names);
    sessions
        .iter()
        .find(|(id, other)| {
            id.as_str() != session_id
                && other.held
                && other.volume_id == this.volume_id
                && other.dir == this.dir
                && sorted(&other.names) == files
        })
        .map(|(id, _)| id.clone())
}

/// Lets eviction take the session again. No-op when it's already gone.
pub(crate) fn release(session_id: &str) {
    if let Some(session) = SESSIONS.lock_ignore_poison().get_mut(session_id) {
        session.held = false;
    }
}

/// A new session over the same folder and the same files, in the session's current order: what
/// the Tags window takes from an open sheet, so closing the sheet never ends the window's
/// session. It reads no media (nothing previews a rename over it).
pub(crate) fn reopen(session_id: &str) -> Result<MultiRenameOpened, MultiRenameError> {
    let (listing_id, volume_id, dir, names) = with_session(session_id, |s| {
        (
            s.listing_id.clone(),
            s.volume_id.clone(),
            s.dir.clone(),
            Arc::clone(&s.names),
        )
    })?;
    // The sheet lists folders too, and a folder named like a track (`x.mp3/`) is no track: leave them out.
    let folders: HashSet<String> = crate::listing_host::listings()
        .entries(&ListingId::new(&listing_id))
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| entry.is_directory)
                .map(|entry| entry.name)
                .collect()
        })
        .unwrap_or_default();
    let names: Arc<[String]> = names.iter().filter(|name| !folders.contains(*name)).cloned().collect();
    Ok(insert_new(listing_id, volume_id, dir, names, &[]))
}

#[cfg(test)]
pub(crate) fn is_held(session_id: &str) -> bool {
    SESSIONS.lock_ignore_poison().get(session_id).is_some_and(|s| s.held)
}
