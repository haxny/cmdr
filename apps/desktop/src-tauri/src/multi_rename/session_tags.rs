//! The Tags window's side of a session (fork): holding one so eviction passes it by, and making
//! one whose media cache starts empty (no EXIF or tag read), from a pane's selection or as a copy
//! of an open sheet's.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use cmdr_collation::NameCollator;

use super::{
    FileEntry, ListingId, ListingLookupError, MultiRenameError, MultiRenameOpened, SESSIONS, insert_new, with_session,
};
use crate::ignore_poison::IgnorePoison;

/// The most audio files a Tags window takes from the folders it was given.
pub(crate) const TAGS_FOLDER_FILE_CAP: usize = 2000;
/// How many levels of subfolders below a given folder a Tags window looks into for audio files.
pub(crate) const TAGS_FOLDER_DEPTH: usize = 8;

/// A Tags window's new session, and how much of the folders it couldn't see.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TagsOpened {
    pub session_id: String,
    /// How many names the session holds.
    pub count: usize,
    /// Places under the selected folders that couldn't be read (or named), so some tracks may be missing.
    pub unreadable: u32,
}

/// What a Tags window was asked to open, read from the pane's listing and not yet walked.
pub(crate) struct TagsPlan {
    listing_id: String,
    volume_id: String,
    dir: PathBuf,
    include_hidden: bool,
    entries: Vec<FileEntry>,
}

/// The listing's side of [`open_for_tags`]: the selected entries and where they are. In memory, quick.
pub(crate) fn plan_for_tags(
    listing_id: &str,
    include_hidden: bool,
    selected: Option<&[usize]>,
    expected_sequence: u64,
) -> Result<TagsPlan, MultiRenameError> {
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
    Ok(TagsPlan {
        listing_id: listing_id.to_string(),
        volume_id,
        dir,
        include_hidden,
        entries,
    })
}

/// The handshake between a walk and the caller waiting for it, so a timeout and a stored session never
/// pass each other: the walk stores its session only while the state is still `Running`, and a caller
/// that gives up after the walk stored it closes that session itself.
pub(crate) struct OpenHandshake(Mutex<OpenState>);

enum OpenState {
    Running,
    /// The walk stored this session.
    Stored(String),
    /// The caller stopped waiting.
    GaveUp,
}

impl OpenHandshake {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(OpenState::Running))
    }

    /// The caller stopped waiting: a walk still running stops, and a session it already stored is closed.
    pub(crate) fn give_up(&self) {
        let previous = std::mem::replace(&mut *self.0.lock_ignore_poison(), OpenState::GaveUp);
        if let OpenState::Stored(session_id) = previous {
            super::close(&session_id);
        }
    }

    fn gave_up(&self) -> bool {
        matches!(*self.0.lock_ignore_poison(), OpenState::GaveUp)
    }

    /// Stores a session with `make` unless the caller gave up first; the state changes under the same lock.
    fn store(&self, make: impl FnOnce() -> MultiRenameOpened) -> Option<MultiRenameOpened> {
        let mut state = self.0.lock_ignore_poison();
        if !matches!(*state, OpenState::Running) {
            return None;
        }
        let opened = make();
        *state = OpenState::Stored(opened.session_id.clone());
        Some(opened)
    }
}

impl TagsPlan {
    /// Whether opening walks the disk (a folder is selected), which can take a while on a share.
    pub(crate) fn has_folder(&self) -> bool {
        self.entries.iter().any(|entry| entry.is_directory)
    }

    /// Walks the folders and stores the session. When the caller gives up (`handshake`), the walk stops
    /// and nothing it stored stays: an abandoned open leaves no session behind.
    pub(crate) fn open(self, handshake: &OpenHandshake) -> Result<TagsOpened, MultiRenameError> {
        let reads_disk = || {
            self.volume_id == crate::tag_editor::session::LOCAL_VOLUME_ID
                || crate::tag_editor::session::volume_gives_local_paths(&self.volume_id)
        };
        let collator = crate::listing_host::listings().collator();
        let (names, unreadable) = names_for_tags(
            &self.entries,
            &self.dir,
            self.include_hidden,
            reads_disk,
            &collator,
            handshake,
        )?;
        let opened = handshake
            .store(|| insert_new(self.listing_id, self.volume_id, self.dir, names.into(), &[]))
            .ok_or(MultiRenameError::TimedOut)?;
        Ok(TagsOpened {
            session_id: opened.session_id,
            count: opened.count,
            unreadable,
        })
    }
}

/// [`super::open`] for a Tags window: the same files, but its media cache starts empty, so no EXIF or
/// tag read is ever queued for it (the window reads tags itself, through `tag_editor`). A selected folder
/// is opened into the audio files under it (subfolders included), named by their path below the
/// listing's folder (`Book/Disc 1/01.mp3`); a plain file is taken as it is.
#[cfg(test)]
pub(crate) fn open_for_tags(
    listing_id: &str,
    include_hidden: bool,
    selected: Option<&[usize]>,
    expected_sequence: u64,
) -> Result<TagsOpened, MultiRenameError> {
    plan_for_tags(listing_id, include_hidden, selected, expected_sequence)?.open(&OpenHandshake::new())
}

fn too_many() -> MultiRenameError {
    MultiRenameError::TooManyTracks {
        limit: u32::try_from(TAGS_FOLDER_FILE_CAP).unwrap_or(u32::MAX),
    }
}

/// A track found under a folder: its session name, and the file it really is (a link's target).
struct Found {
    name: String,
    real: PathBuf,
    is_link: bool,
}

/// A name the session may take, with the file it really is when that was looked up.
struct Candidate {
    name: String,
    real: Option<PathBuf>,
    is_link: bool,
}

/// The session names for the selected `entries` in `dir`, in selection order, and how many places under
/// the folders couldn't be read: a plain file by its name, a folder by the audio files under it (sorted
/// by `collator` on their relative names). A file reached twice is named once, by name or by the file it
/// really is: a link to a file the selection takes anyway (wherever that sits in it) is left out.
/// `reads_disk` says whether the folder's volume gives `std::fs` its files; it is asked only when a
/// folder is selected.
fn names_for_tags(
    entries: &[FileEntry],
    dir: &Path,
    include_hidden: bool,
    reads_disk: impl FnOnce() -> bool,
    collator: &NameCollator,
    handshake: &OpenHandshake,
) -> Result<(Vec<String>, u32), MultiRenameError> {
    let has_folder = entries.iter().any(|entry| entry.is_directory);
    if has_folder && !reads_disk() {
        return Err(MultiRenameError::FoldersNeedLocalDisk);
    }
    let real_dir = if has_folder {
        dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf())
    } else {
        dir.to_path_buf()
    };
    let mut candidates: Vec<Candidate> = Vec::with_capacity(entries.len());
    let mut walked = 0usize;
    let mut unreadable = 0usize;
    for entry in entries {
        if !entry.is_directory {
            // Only a selection with folders can reach one file twice by different names.
            let real = has_folder
                .then(|| std::fs::canonicalize(dir.join(&entry.name)).ok())
                .flatten();
            candidates.push(Candidate {
                name: entry.name.clone(),
                real,
                is_link: false,
            });
            continue;
        }
        let (mut found, skipped) = audio_files_under(dir, &real_dir, &entry.name, include_hidden, handshake)?;
        unreadable += skipped;
        found.sort_by(|a, b| collator.compare(&a.name, &b.name));
        walked += found.len();
        if walked > TAGS_FOLDER_FILE_CAP {
            return Err(too_many());
        }
        candidates.extend(found.into_iter().map(|file| Candidate {
            name: file.name,
            real: Some(file.real),
            is_link: file.is_link,
        }));
    }
    // Real files first, so a link never shadows the file it points at, wherever each sits in the selection.
    let mut real_files: HashSet<PathBuf> = candidates
        .iter()
        .filter(|c| !c.is_link)
        .filter_map(|c| c.real.clone())
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut names = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if candidate.is_link && candidate.real.is_some_and(|real| !real_files.insert(real)) {
            continue;
        }
        if seen.insert(candidate.name.clone()) {
            names.push(candidate.name);
        }
    }
    if unreadable > 0 {
        log::debug!(target: "multi_rename::session_tags", "Tags window: places under a selected folder that couldn't be read or named: {unreadable}");
    }
    Ok((names, u32::try_from(unreadable).unwrap_or(u32::MAX)))
}

/// The audio files below `dir/folder` (to [`TAGS_FOLDER_DEPTH`] levels, symlinked folders not followed,
/// AppleDouble `._*` files never), and how many places that may hold tracks couldn't be read or named.
/// `real_dir` is `dir` with its links resolved. Stops with `TooManyTracks` as soon as one folder alone is
/// over the cap, and with `TimedOut` once the caller gave up.
fn audio_files_under(
    dir: &Path,
    real_dir: &Path,
    folder: &str,
    include_hidden: bool,
    handshake: &OpenHandshake,
) -> Result<(Vec<Found>, usize), MultiRenameError> {
    let walk = walkdir::WalkDir::new(dir.join(folder))
        .max_depth(TAGS_FOLDER_DEPTH + 1) // the files of the deepest folder sit one level lower
        .follow_links(false)
        .follow_root_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0 || (!name.starts_with("._") && (include_hidden || !name.starts_with('.')))
        });
    let mut found = Vec::new();
    let mut skipped = 0usize;
    for entry in walk {
        if handshake.gave_up() {
            return Err(MultiRenameError::TimedOut);
        }
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let Some(name) = entry.path().strip_prefix(dir).ok().and_then(Path::to_str) else {
            // A name that isn't UTF-8 can't be a session name: a missing track only if it is audio.
            let lossy = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir() || cmdr_tags::format_of(&lossy).is_some() {
                skipped += 1;
            }
            continue;
        };
        if entry.depth() == 0 || cmdr_tags::format_of(name).is_none() {
            continue;
        }
        let kind = entry.file_type();
        let is_link = kind.is_symlink();
        let real = if is_link {
            // A link to a file is a track; to anything else, or to nothing, it is not.
            match std::fs::canonicalize(entry.path()) {
                Ok(real) if real.is_file() => real,
                _ => continue,
            }
        } else if kind.is_file() {
            real_dir.join(name)
        } else {
            continue;
        };
        found.push(Found {
            name: name.to_string(),
            real,
            is_link,
        });
        if found.len() > TAGS_FOLDER_FILE_CAP {
            return Err(too_many());
        }
    }
    Ok((found, skipped))
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
