//! Audiobookshelf's side files: `reader.txt` (its first line is the narrator) and `desc.txt` (the
//! description), written into the one book folder ([`book_folder`]) with the tags when the user
//! turned `multiRename.tagEditor.absSideFiles` on (`ApplyRequest::side_files`). Audiobookshelf reads
//! the narrator only from Composer or `reader.txt`, never from Performer, and the user's canon keeps
//! the narrator in Performer, so the side file carries it.
//!
//! What a side file says comes from the session's tracks as edited ([`wanted`]): the narrator is the
//! tracks' Performer values, distinct, joined with `, `, and only when every track names the same
//! ones; `desc.txt` is the Description every track shares. Tracks that disagree give no file, and
//! the change list says why ([`SideFileChange::Withheld`]).
//!
//! Data safety: a missing file is created; one that already says the same is left untouched; one
//! that says something else is replaced only when the change list showed that replacement with that
//! old text (`ApplyRequest::allow_side_files`), else it is held back (`TagApplyReport::side_files_held`).
//! A write goes to a temporary file in the same folder and is renamed over the name; ❌ a symlink or
//! anything but a regular file at the name is never read, followed, or replaced. Each write keeps the
//! old content (or that there was none) under the apply's id, so the write's undo puts it back or
//! removes the created file, only while the file still holds what was written ([`undo`]).
//!
//! Lock order: [`UNDO`] is taken under the undo store, and nothing is taken under it.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use cmdr_tags::{FileTags, RestoreReport, TagError, TagField};

use super::apply::shown;
use super::session::{TagEditorError, require_editable, with_session};
use crate::ignore_poison::IgnorePoison;

const LOG_TARGET: &str = "tag_editor::side_files";

/// The longest side file read or written: a narrator line, a description of a few pages.
pub(crate) const SIDE_FILE_LIMIT: u64 = 64 * 1024;

/// Which side file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SideFile {
    /// `reader.txt`: the narrator.
    Reader,
    /// `desc.txt`: the description.
    Desc,
}

impl SideFile {
    pub(crate) fn file_name(self) -> &'static str {
        match self {
            Self::Reader => "reader.txt",
            Self::Desc => "desc.txt",
        }
    }

    /// Whether the file there already says `text`: Audiobookshelf reads `reader.txt`'s first line
    /// and all of `desc.txt`.
    fn says(self, existing: &str, text: &str) -> bool {
        match self {
            Self::Reader => existing.lines().next().unwrap_or_default().trim() == text,
            Self::Desc => existing.trim_end() == text.trim_end(),
        }
    }
}

/// Why a side file isn't written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SideFileWithheld {
    /// The tracks don't all name the same narrators (or share one description).
    TracksDisagree,
    /// The text is longer than a side file may be.
    TooLong,
    /// Something other than a regular file has the name (a link, a folder).
    NotAFile,
    /// The file there is too large or can't be read, so its content couldn't be kept for undo.
    Unreadable,
    /// The tracks come from more than one book folder, so no folder is the book's.
    NotOneBook,
}

/// What a write would do with one side file. A file that already says the same isn't listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SideFileChange {
    Create {
        file: SideFile,
        path: String,
        text: String,
    },
    /// `old` is the file's text now (read as UTF-8, lossy); the write replaces it only when the
    /// request names this path with this `old` (`ApplyRequest::allow_side_files`).
    Replace {
        file: SideFile,
        path: String,
        old: String,
        text: String,
    },
    Withheld {
        file: SideFile,
        reason: SideFileWithheld,
    },
}

/// A replacement the change list showed: the path and the old text it showed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SideFileSeen {
    pub path: String,
    pub old: String,
    pub text: String,
}

/// What the book's tracks say a side file should hold: `None` when they say nothing (no narrator,
/// no description), else the text or why there's none.
pub(super) fn wanted(tracks: &[FileTags]) -> Vec<(SideFile, Result<String, SideFileWithheld>)> {
    [
        (SideFile::Reader, agreed(tracks, TagField::Performer, ", ")),
        (SideFile::Desc, agreed(tracks, TagField::Description, "\n")),
    ]
    .into_iter()
    .filter_map(|(file, text)| Some((file, text?)))
    .map(|(file, text)| {
        let text = text.and_then(|text| {
            if text.len() as u64 >= SIDE_FILE_LIMIT {
                Err(SideFileWithheld::TooLong)
            } else {
                Ok(text)
            }
        });
        (file, text)
    })
    .collect()
}

/// The field's values, distinct and trimmed, joined with `sep`, when every track has the same ones
/// (in any order); `None` when no track has any.
fn agreed(tracks: &[FileTags], field: TagField, sep: &str) -> Option<Result<String, SideFileWithheld>> {
    let lists: Vec<Vec<&str>> = tracks.iter().map(|tags| distinct(tags, field)).collect();
    let first = lists.first()?.clone();
    if lists.iter().all(Vec::is_empty) {
        return None;
    }
    let mut sorted_first = first.clone();
    sorted_first.sort_unstable();
    let agree = lists.iter().all(|list| {
        let mut sorted = list.clone();
        sorted.sort_unstable();
        sorted == sorted_first
    });
    Some(if agree {
        Ok(first.join(sep))
    } else {
        Err(SideFileWithheld::TracksDisagree)
    })
}

fn distinct(tags: &FileTags, field: TagField) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for value in tags.fields.get(&field).into_iter().flatten() {
        let value = value.trim();
        if !value.is_empty() && !seen.contains(&value) {
            seen.push(value);
        }
    }
    seen
}

/// The bytes a side file holding `text` is written with.
fn content_of(text: &str) -> Vec<u8> {
    format!("{}\n", text.trim_end()).into_bytes()
}

/// A side file's name in the book folder, as it is now.
enum OnDisk {
    Missing,
    /// A regular file, with its bytes.
    File(Vec<u8>),
    Withheld(SideFileWithheld),
}

/// Reads what is at `path`: never through a symlink, never a FIFO, never past the limit.
fn on_disk(path: &Path) -> OnDisk {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return OnDisk::Missing,
        Err(_) => return OnDisk::Withheld(SideFileWithheld::Unreadable),
    };
    if !meta.file_type().is_file() {
        return OnDisk::Withheld(SideFileWithheld::NotAFile);
    }
    if meta.len() > SIDE_FILE_LIMIT {
        return OnDisk::Withheld(SideFileWithheld::Unreadable);
    }
    match read_no_follow(path) {
        Ok(bytes) => OnDisk::File(bytes),
        Err(_) => OnDisk::Withheld(SideFileWithheld::Unreadable),
    }
}

/// The file's bytes, opened with `O_NOFOLLOW | O_NONBLOCK` and checked again on the open file, at
/// most [`SIDE_FILE_LIMIT`].
fn read_no_follow(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > SIDE_FILE_LIMIT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a small regular file"));
    }
    let mut bytes = Vec::new();
    file.take(SIDE_FILE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > SIDE_FILE_LIMIT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "grew past the limit"));
    }
    Ok(bytes)
}

/// A new temporary file beside `path` holding `bytes`, synced, never opened through a link. It is
/// removed again when this fails.
fn temp_beside(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no folder"));
    };
    let tmp = dir.join(format!(
        ".{}.cmdr-{}-{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    match write_new(&tmp, bytes) {
        Ok(()) => Ok(tmp),
        Err(error) => {
            discard(&tmp);
            Err(error)
        }
    }
}

/// A new file at `tmp` (never through a link) holding `bytes`, synced.
fn write_new(tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW)
        .open(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Best effort: a temp file left behind is hidden and harmless; the write's own answer is what counts.
fn discard(tmp: &Path) {
    if let Err(error) = std::fs::remove_file(tmp) {
        log::warn!(target: LOG_TARGET, "Couldn't remove a side file's temporary copy: {:?}", error.kind());
    }
}

/// Creates `path` holding `bytes`, whole or not at all. ❌ Never replaces anything: a file (or link)
/// that appeared at the name since the check makes it `AlreadyExists` (a hard link, unlike a
/// rename, refuses an existing name).
pub(super) fn create_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_beside(path, bytes)?;
    let linked = std::fs::hard_link(&tmp, path);
    discard(&tmp);
    linked
}

/// Replaces `path` with `bytes` only while it still holds exactly `expected` (read again right
/// before, never through a link): `Ok(false)` when it changed since. The new file keeps the old
/// one's permissions.
pub(super) fn replace_atomic(path: &Path, bytes: &[u8], expected: &[u8]) -> io::Result<bool> {
    let tmp = temp_beside(path, bytes)?;
    let replaced = swap_in(&tmp, path, expected);
    if !matches!(replaced, Ok(true)) {
        discard(&tmp);
    }
    replaced
}

/// [`replace_atomic`]'s last steps: the old permissions onto `tmp`, the old bytes checked, the rename.
fn swap_in(tmp: &Path, path: &Path, expected: &[u8]) -> io::Result<bool> {
    let permissions = std::fs::symlink_metadata(path)?.permissions();
    std::fs::set_permissions(tmp, permissions)?;
    if read_no_follow(path)? != expected {
        return Ok(false);
    }
    std::fs::rename(tmp, path).map(|()| true)
}

/// Which copy of the tracks' tags a [`Book`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BookTags {
    /// As edited: what a write would make them (the change list).
    Edited,
    /// As on disk: what the files say (right after a write, what the side files may say).
    OnDisk,
}

/// The book a session writes side files for: its folder ([`book_folder`], `None` when the tracks
/// come from more than one book) and its readable tracks' tags, in current order.
pub(super) struct Book {
    pub folder: Option<PathBuf>,
    pub tracks: Vec<FileTags>,
}

/// Whether a folder's name is a disc's: `CD 1`, `CD1`, `Disc 2`, `disk 3` (any case, digits last).
fn is_disc_folder(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["cd", "disc", "disk"].iter().any(|prefix| {
        lower.strip_prefix(prefix).is_some_and(|rest| {
            let digits = rest.trim_start_matches(' ');
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

/// `folder`, or its parent when `folder` is a disc folder ([`is_disc_folder`]): the book it belongs to.
fn book_above(folder: PathBuf) -> PathBuf {
    match (folder.file_name().and_then(|name| name.to_str()), folder.parent()) {
        (Some(name), Some(parent)) if is_disc_folder(name) => parent.to_path_buf(),
        _ => folder,
    }
}

/// The one book folder the session's audio `names` (paths below the listing folder `dir`) belong
/// to: the folder the session was opened on (a name's first part, `Book/Disc 1/01.mp3`), or `dir`
/// for plain files; a disc folder's book is its parent. `None` when the names come from more than
/// one book (two folders, or a folder and loose files): side files are then withheld
/// ([`SideFileWithheld::NotOneBook`]). Covers keep `cover::tracks_folder`.
pub(super) fn book_folder(dir: &Path, names: &[String]) -> Option<PathBuf> {
    let mut books = names
        .iter()
        .filter(|name| cmdr_tags::format_of(name).is_some())
        .map(|name| {
            let mut parts = Path::new(name).components();
            match (parts.next(), parts.next()) {
                (Some(first), Some(_)) => book_above(dir.join(first)),
                _ => book_above(dir.to_path_buf()),
            }
        });
    let Some(first) = books.next() else {
        return Some(book_above(dir.to_path_buf()));
    };
    books.all(|book| book == first).then_some(first)
}

/// The session's [`Book`], or `None` when it isn't editable (side files go only where tags do).
/// Copies only: no file is touched.
pub(super) fn book_of(session_id: &str, copy: BookTags) -> Result<Option<Book>, TagEditorError> {
    with_session(session_id, |layout, session| {
        if require_editable(layout).is_err() {
            return Ok(None);
        }
        let names: Vec<String> = layout
            .tracks
            .iter()
            .filter_map(|path| path.strip_prefix(&layout.dir).ok())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        let tracks = layout
            .tracks
            .iter()
            .filter_map(|path| session.by_path.get(path)?.loaded())
            .map(|loaded| match copy {
                BookTags::Edited => loaded.edited.clone(),
                BookTags::OnDisk => loaded.disk.clone(),
            })
            .collect();
        Ok(Some(Book {
            folder: book_folder(&layout.dir, &names),
            tracks,
        }))
    })
}

/// What a write would do with the book's side files; none for a read-only session. The files are
/// read after the lock.
pub fn side_file_changes(session_id: &str) -> Result<Vec<SideFileChange>, TagEditorError> {
    Ok(book_of(session_id, BookTags::Edited)?
        .map(|book| changes(book.folder.as_deref(), &book.tracks))
        .unwrap_or_default())
}

/// The change list's side-file rows for the book in `folder` (`None`: more than one book, so
/// each file the tracks ask for is withheld), from its tracks as edited.
/// Reads the folder: ❌ not under a lock.
pub(super) fn changes(folder: Option<&Path>, tracks: &[FileTags]) -> Vec<SideFileChange> {
    wanted(tracks)
        .into_iter()
        .filter_map(|(file, text)| {
            let text = match text {
                Ok(text) => text,
                Err(reason) => return Some(SideFileChange::Withheld { file, reason }),
            };
            let Some(folder) = folder else {
                return Some(SideFileChange::Withheld {
                    file,
                    reason: SideFileWithheld::NotOneBook,
                });
            };
            let path = folder.join(file.file_name());
            match on_disk(&path) {
                OnDisk::Missing => Some(SideFileChange::Create {
                    file,
                    path: shown(&path),
                    text,
                }),
                OnDisk::File(bytes) => {
                    let old = String::from_utf8_lossy(&bytes).into_owned();
                    (!file.says(&old, &text)).then(|| SideFileChange::Replace {
                        file,
                        path: shown(&path),
                        old,
                        text,
                    })
                }
                OnDisk::Withheld(reason) => Some(SideFileChange::Withheld { file, reason }),
            }
        })
        .collect()
}

/// A side file a write wrote: what it held before (`None`: there was none) and what was written.
#[derive(Debug, Clone)]
struct Undoable {
    path: PathBuf,
    before: Option<Vec<u8>>,
    written: Vec<u8>,
}

/// What [`write`] did, by path as the view shows it.
#[derive(Debug, Default)]
pub(super) struct SideFilesWritten {
    pub written: Vec<String>,
    /// Not replaced: the request didn't name this replacement (or the file changed since the list).
    pub held: Vec<String>,
    pub failed: Vec<String>,
    undo: Vec<Undoable>,
}

impl SideFilesWritten {
    /// Keeps what undoes these writes under `apply_id`; a write without an undo keeps nothing.
    pub(super) fn keep_undo(self, apply_id: Option<&str>) {
        if let Some(apply_id) = apply_id
            && !self.undo.is_empty()
        {
            remember(apply_id, self.undo);
        }
    }
}

/// Writes the side files the tracks ask for into `folder`, each checked again right before: a file
/// that already says the same is left alone, a different one is replaced only when `allowed` names
/// its path with its current text and the new text, and only when this write keeps an undo
/// (`undoable`; else it is held: a user's file is never replaced for good). A missing one is
/// created. Blocking file work.
pub(super) fn write(folder: &Path, tracks: &[FileTags], allowed: &[SideFileSeen], undoable: bool) -> SideFilesWritten {
    let mut done = SideFilesWritten::default();
    for (file, text) in wanted(tracks) {
        let Ok(text) = text else { continue };
        let path = folder.join(file.file_name());
        let content = content_of(&text);
        let before = match on_disk(&path) {
            OnDisk::Missing => None,
            OnDisk::File(bytes) => {
                let old = String::from_utf8_lossy(&bytes);
                if file.says(&old, &text) {
                    continue;
                }
                let seen = allowed
                    .iter()
                    .any(|seen| seen.path == shown(&path) && seen.old == old && seen.text == text);
                if !seen || !undoable {
                    done.held.push(shown(&path));
                    continue;
                }
                Some(bytes)
            }
            OnDisk::Withheld(_) => continue,
        };
        let written = match &before {
            None => create_atomic(&path, &content).map(|()| true),
            Some(old) => replace_atomic(&path, &content, old),
        };
        match written {
            Ok(true) => {
                done.written.push(shown(&path));
                done.undo.push(Undoable {
                    path,
                    before,
                    written: content,
                });
            }
            // Changed (or appeared) since the check: as unseen as any other.
            Ok(false) => done.held.push(shown(&path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => done.held.push(shown(&path)),
            Err(error) => {
                log::warn!(target: LOG_TARGET, "Couldn't write a side file: {:?}", error.kind());
                done.failed.push(shown(&path));
            }
        }
    }
    done
}

/// The side files each apply wrote, newest last, at most as many applies as the undo store keeps.
/// One apply's side files to put back: its id and what each held.
type KeptUndo = (String, Vec<Undoable>);

static UNDO: LazyLock<Mutex<VecDeque<KeptUndo>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

fn remember(apply_id: &str, undo: Vec<Undoable>) {
    let mut kept = UNDO.lock_ignore_poison();
    kept.push_back((apply_id.to_owned(), undo));
    while kept.len() > cmdr_tags::UndoLimits::default().max_applies {
        kept.pop_front();
    }
}

/// Puts back the side files apply `apply_id` wrote, adding each to `report`: the old content back
/// (or the created file removed) while the file still holds what was written, else left alone as
/// `changed`. One that fails stays undoable.
pub(super) fn undo(apply_id: &str, report: &mut RestoreReport) {
    let taken = {
        let mut kept = UNDO.lock_ignore_poison();
        kept.iter()
            .position(|(id, _)| id == apply_id)
            .and_then(|at| kept.remove(at))
    };
    let Some((_, entries)) = taken else { return };
    let mut left = Vec::new();
    for entry in entries {
        match read_no_follow(&entry.path) {
            Ok(now) if now == entry.written => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                report.missing.push(entry.path);
                continue;
            }
            _ => {
                report.changed.push(entry.path);
                continue;
            }
        }
        let restored = match &entry.before {
            Some(before) => replace_atomic(&entry.path, before, &entry.written),
            None => std::fs::remove_file(&entry.path).map(|()| true),
        };
        match restored {
            Ok(true) => report.restored.push(entry.path),
            Ok(false) => report.changed.push(entry.path),
            Err(error) => {
                log::warn!(target: LOG_TARGET, "Couldn't put a side file back: {:?}", error.kind());
                report.failed.push((
                    entry.path.clone(),
                    TagError::Io(error.kind(), "the side file couldn't be put back".to_owned()),
                ));
                left.push(entry);
            }
        }
    }
    if !left.is_empty() {
        remember(apply_id, left);
    }
}
