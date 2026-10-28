//! Every distinct cover of the book: which ones the readable files carry and how many carry each,
//! the image of one, and the in-memory edits that give every file one cover (a choice, a pasted
//! picture, a dropped file). Nothing here writes a file: a cover edit is a change Write lists.
//!
//! A cover is named by its key (`Loaded::cover_key`, taken once per read or new cover, never
//! here), so grouping hashes nothing. Unreadable files are never written, so they count in no
//! choice.

use cmdr_tags::Cover;
use cmdr_tags::shared::CoverEdit;
use serde::Serialize;

use super::cover::thumbnail_url;
use super::session::{Loaded, TagEdit, TagEditorError, edit, with_loaded};
use super::view::{TagEditorView, common_cover, cover_key, pixel_size};

/// The key of the choice that stands for the files without a cover.
pub const NO_COVER_KEY: &str = "none";

/// One distinct cover across the readable files, or (`key: "none"`, empty `mime`, 0 bytes) the
/// files that have none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoverChoice {
    /// Names the cover in `get_cover_image` and `use_cover_for_all`: a hash of its bytes.
    pub key: String,
    pub mime: String,
    pub bytes: u64,
    /// Pixel size, when the image header could be read.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// How many readable files carry it.
    pub files: u32,
}

/// One distinct cover while grouping: its first carrier's cover, key, and count.
struct Group<'a> {
    cover: &'a Cover,
    key: u64,
    files: u32,
}

/// The key a file's edited cover goes by: the cached one, else (never in practice: every read and
/// store keys it) a hash.
fn key_of(file: &Loaded, cover: &Cover) -> u64 {
    file.cover_key.unwrap_or_else(|| cover_key(cover))
}

fn key_text(key: u64) -> String {
    format!("{key:016x}")
}

/// The distinct covers of `files`, the most carried first (ties in file order), then the files
/// without one. Same MIME, size, and key is one cover. Pure; reads no file.
pub(super) fn choices_of(files: &[&Loaded]) -> Vec<CoverChoice> {
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut uncovered = 0u32;
    for file in files {
        let Some(cover) = &file.edited.cover else {
            uncovered += 1;
            continue;
        };
        let key = key_of(file, cover);
        match groups.iter_mut().find(|group| {
            group.key == key && group.cover.mime == cover.mime && group.cover.data.len() == cover.data.len()
        }) {
            Some(group) => group.files += 1,
            None => groups.push(Group { cover, key, files: 1 }),
        }
    }
    // Stable: equal counts keep file order.
    groups.sort_by_key(|group| std::cmp::Reverse(group.files));
    let covered = groups.into_iter().map(|group| {
        let (width, height) = pixel_size(group.cover).unzip();
        CoverChoice {
            key: key_text(group.key),
            mime: group.cover.mime.clone(),
            bytes: u64::try_from(group.cover.data.len()).unwrap_or(u64::MAX),
            width,
            height,
            files: group.files,
        }
    });
    let none = (uncovered > 0).then(|| CoverChoice {
        key: NO_COVER_KEY.to_string(),
        mime: String::new(),
        bytes: 0,
        width: None,
        height: None,
        files: uncovered,
    });
    covered.chain(none).collect()
}

/// What `key` names among `files`: `Some(None)` for "none", `Some(Some(cover))` for a cover a file
/// carries, `None` when no file carries it.
fn find(files: &[&Loaded], key: &str) -> Option<Option<Cover>> {
    if key == NO_COVER_KEY {
        return Some(None);
    }
    files.iter().find_map(|file| {
        let cover = file.edited.cover.as_ref()?;
        (key_text(key_of(file, cover)) == key).then(|| Some(cover.clone()))
    })
}

/// The session's cover choices. The covers are copied out under the lock (shared buffers, no
/// bytes copied); their pixel sizes are read after it.
pub fn list(session_id: &str) -> Result<Vec<CoverChoice>, TagEditorError> {
    let owned: Vec<Loaded> = with_loaded(session_id, |files| files.iter().map(|file| (*file).clone()).collect())?;
    let files: Vec<&Loaded> = owned.iter().collect();
    Ok(choices_of(&files))
}

/// The cover every readable file shares (its edited tags), or `None` when no file has one, there
/// are no readable files, or the files' covers differ. Needs an open session.
pub fn shared_cover(session_id: &str) -> Result<Option<Cover>, TagEditorError> {
    with_loaded(session_id, |files| common_cover(files).cloned())
}

/// A cover as a `data:` URL: the one `key` names (`None` for "none", `UnknownCover` when no file
/// carries it), or for no key the one every file shares. `None` too when the bytes aren't a JPEG,
/// PNG, or WebP image.
pub fn image_of(session_id: &str, key: Option<&str>) -> Result<Option<String>, TagEditorError> {
    let cover = match key {
        None => shared_cover(session_id)?,
        Some(key) => with_loaded(session_id, |files| find(files, key))?.ok_or(TagEditorError::UnknownCover)?,
    };
    Ok(cover.and_then(|cover| thumbnail_url(&cover.data)))
}

/// Gives every readable file the cover `key` names ("none" takes every cover out), in memory.
/// A key no file carries any more refuses (`UnknownCover`) and changes nothing.
pub fn use_for_all(session_id: &str, key: &str) -> Result<TagEditorView, TagEditorError> {
    let chosen = with_loaded(session_id, |files| find(files, key))?.ok_or(TagEditorError::UnknownCover)?;
    let cover_edit = match chosen {
        Some(cover) => CoverEdit::Set(cover),
        None => CoverEdit::Remove,
    };
    edit(session_id, vec![TagEdit::Cover { edit: cover_edit }])
}

/// Gives every readable file `cover` (a pasted or dropped picture), in memory.
pub fn set_for_all(session_id: &str, cover: Cover) -> Result<TagEditorView, TagEditorError> {
    edit(
        session_id,
        vec![TagEdit::Cover {
            edit: CoverEdit::Set(cover),
        }],
    )
}
