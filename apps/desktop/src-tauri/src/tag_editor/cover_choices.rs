//! Every distinct cover of the book: which ones the readable files carry and how many carry each,
//! the image of one, and the in-memory edits that give every file one cover (a choice, a pasted
//! picture, a dropped file). Nothing here writes a file: a cover edit is a change Write lists.
//!
//! A cover is named by its key (`Loaded::cover_key`, taken once per read or new cover, never
//! here), so grouping by bytes hashes nothing. Covers that look the same (one picture re-encoded or
//! resized, so different bytes) are then one choice too: see [`NEAR_DUPLICATE_BITS`]. Unreadable
//! files are never written, so they count in no choice.

use std::path::{Path, PathBuf};
use std::time::Duration;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use cmdr_fs::deadline::{StallWatch, blocking_typed_result_until_stalled, blocking_typed_result_with_timeout};
use cmdr_tags::Cover;
use cmdr_tags::shared::CoverEdit;
use serde::Serialize;
use tokio::sync::Semaphore;

use super::cover::{cover_from_bytes, thumbnail_url};
use super::cover_image::{dhash_of, hamming, thumbnail_of};
use super::session::{Loaded, TagEdit, TagEditorError, edit, with_loaded};
use super::view::{TagEditorView, common_cover, cover_key, pixel_size};

/// The key of the choice that stands for the files without a cover.
pub const NO_COVER_KEY: &str = "none";

/// Two covers whose 64-bit dHashes (`cover_image::dhash_of`) differ in at most this many bits look
/// the same and are one choice. Copies of one picture land at 0 bits (JPEG quality 90 against 70,
/// and 30 at 200 px; 600 px against 500 px; PNG against JPEG: the pictures in `cover_near_test.rs`;
/// and the 24 distinct re-encoded copies of one real audiobook's cover, all 0 bits apart), while
/// different pictures land at 29 or more (the same layout mirrored: 60). Five keeps a wide margin
/// on both sides (verified with `image` 0.25.10, the generated pictures and one real MP3 book, 2026-10-29).
pub const NEAR_DUPLICATE_BITS: u32 = 5;

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
    /// How many readable files carry it, or one that looks the same.
    pub files: u32,
    /// How many other distinct covers look the same as this one and are counted in it (0 when
    /// none): this one is the largest of them in pixels.
    pub near_duplicates: u32,
}

/// One readable file's edited cover and its key, copied out of the session (a shared buffer, no bytes copied).
pub(super) struct FileCover {
    cover: Option<Cover>,
    key: Option<u64>,
}

/// Each file's cover and key: all `list` needs, so it never copies the files' tags.
pub(super) fn covers_of(files: &[&Loaded]) -> Vec<FileCover> {
    files
        .iter()
        .map(|file| FileCover {
            cover: file.edited.cover.clone(),
            key: file.cover_key,
        })
        .collect()
}

/// One distinct cover while grouping: its first carrier's cover, key, and count.
struct Group<'a> {
    cover: &'a Cover,
    key: u64,
    files: u32,
}

/// Distinct covers that look the same, while merging: the largest one's cover and key stand for
/// them (`face`), compared by the first one's hash (`seed`, so likeness never chains on).
struct Alike<'a> {
    face: Group<'a>,
    size: Option<(u32, u32)>,
    seed: Option<u64>,
    files: u32,
    merged: u32,
}

/// The key a cover goes by: the cached one, else (never in practice: every read and store keys
/// it) a hash.
fn key_or_hash(key: Option<u64>, cover: &Cover) -> u64 {
    key.unwrap_or_else(|| cover_key(cover))
}

fn key_text(key: u64) -> String {
    format!("{key:016x}")
}

fn area(size: Option<(u32, u32)>) -> u64 {
    size.map_or(0, |(width, height)| u64::from(width) * u64::from(height))
}

/// The distinct covers of `files` by bytes (same MIME, size, and key), in file order, and how many
/// files have none.
fn exact_groups(files: &[FileCover]) -> (Vec<Group<'_>>, u32) {
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut uncovered = 0u32;
    for file in files {
        let Some(cover) = &file.cover else {
            uncovered += 1;
            continue;
        };
        let key = key_or_hash(file.key, cover);
        match groups.iter_mut().find(|group| {
            group.key == key && group.cover.mime == cover.mime && group.cover.data.len() == cover.data.len()
        }) {
            Some(group) => group.files += 1,
            None => groups.push(Group { cover, key, files: 1 }),
        }
    }
    (groups, uncovered)
}

/// `groups` with the ones that look the same as one: the largest in pixels stands (the first on a
/// tie), the files summed. A cover that doesn't decode has no hash and stays alone.
fn merge_alike(groups: Vec<Group<'_>>) -> Vec<Alike<'_>> {
    let mut merged: Vec<Alike<'_>> = Vec::new();
    for group in groups {
        let hash = dhash_of(group.cover, &key_text(group.key));
        let size = pixel_size(group.cover);
        let near = hash.and_then(|hash| {
            merged.iter_mut().find(|alike| {
                alike
                    .seed
                    .is_some_and(|seed| hamming(seed, hash) <= NEAR_DUPLICATE_BITS)
            })
        });
        match near {
            Some(alike) => {
                alike.files += group.files;
                alike.merged += 1;
                if area(size) > area(alike.size) {
                    alike.face = group;
                    alike.size = size;
                }
            }
            None => merged.push(Alike {
                files: group.files,
                face: group,
                size,
                seed: hash,
                merged: 0,
            }),
        }
    }
    merged
}

/// The distinct covers of `files`, the most carried first (ties in file order), then the files
/// without one. Same MIME, size, and key is one cover, and so are covers that look the same.
/// Reads no file; decodes each distinct cover once (`dhash_of` caches), so call it off any lock.
pub(super) fn choices_of(files: &[FileCover]) -> Vec<CoverChoice> {
    let (groups, uncovered) = exact_groups(files);
    let mut merged = merge_alike(groups);
    // Stable: equal counts keep file order.
    merged.sort_by_key(|alike| std::cmp::Reverse(alike.files));
    let covered = merged.into_iter().map(|alike| {
        let (width, height) = alike.size.unzip();
        CoverChoice {
            key: key_text(alike.face.key),
            mime: alike.face.cover.mime.clone(),
            bytes: u64::try_from(alike.face.cover.data.len()).unwrap_or(u64::MAX),
            width,
            height,
            files: alike.files,
            near_duplicates: alike.merged,
        }
    });
    let none = (uncovered > 0).then(|| CoverChoice {
        key: NO_COVER_KEY.to_string(),
        mime: String::new(),
        bytes: 0,
        width: None,
        height: None,
        files: uncovered,
        near_duplicates: 0,
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
        (key_text(key_or_hash(file.cover_key, cover)) == key).then(|| Some(cover.clone()))
    })
}

/// The session's cover choices. Only each file's cover and key are copied out under the lock;
/// the pixel sizes and the likeness hashes are read after it.
pub fn list(session_id: &str) -> Result<Vec<CoverChoice>, TagEditorError> {
    let covers = with_loaded(session_id, covers_of)?;
    Ok(choices_of(&covers))
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

/// A small PNG `data:` URL of the cover `key` names, for a tile (`cover_image::thumbnail_of`,
/// cached by key); `None` for "none" or a cover that doesn't decode, `UnknownCover` for a stale key.
pub fn thumbnail(session_id: &str, key: &str) -> Result<Option<String>, TagEditorError> {
    let cover = with_loaded(session_id, |files| find(files, key))?.ok_or(TagEditorError::UnknownCover)?;
    Ok(cover.and_then(|cover| thumbnail_of(&cover, key)))
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

/// How many dropped-file reads may run at once. A read given up on a dead mount stays parked on
/// a blocking thread until the mount answers; past this many, a new drop is refused at once
/// (`CoverBusy`) instead of parking another thread.
pub const DROP_READS_AT_ONCE: usize = 2;
/// The app's permits for dropped-file reads ([`DROP_READS_AT_ONCE`]); tests pass their own.
pub static DROP_READS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(DROP_READS_AT_ONCE)));

/// How far a dropped file's read got: bytes read, and whether it ended.
#[derive(Default)]
struct DropProgress {
    bytes: AtomicU64,
    done: AtomicBool,
}

/// Watches a dropped file's read by its bytes: idle is the time since the count last moved.
struct ProgressWatch {
    progress: Arc<DropProgress>,
    seen: u64,
    moved: tokio::time::Instant,
}

impl StallWatch<TagEditorError> for ProgressWatch {
    fn idle_for(&self) -> Duration {
        self.moved.elapsed()
    }

    fn on_poll(&mut self) {
        let now = self.progress.bytes.load(Ordering::Relaxed);
        if now != self.seen {
            self.seen = now;
            self.moved = tokio::time::Instant::now();
        }
    }

    fn give_up(&mut self) -> Option<TagEditorError> {
        // A read that ended a moment ago has its answer on the way: take it.
        (!self.progress.done.load(Ordering::Acquire)).then_some(TagEditorError::CoverTimedOut)
    }
}

/// Gives every readable file the picture file at `path` (a drop), in memory, read by `read`
/// (`cover::read_dropped_counted`, which counts its bytes into the second argument). A drop can
/// come from a NAS that is slow, or one that stopped answering: the read may take as long as it
/// keeps moving, and is given up once its byte count stood still for `stall_limit`
/// (`CoverTimedOut`). The edit is made only from the awaited answer, so a read given up on and
/// finishing later changes nothing. Each read holds a permit of `reads` (the app's is
/// [`DROP_READS`]) until it ends; with none free a drop is refused as busy (the reads before
/// it are stuck), without reading.
pub async fn set_from_path_within(
    session_id: String,
    path: PathBuf,
    stall_limit: Duration,
    reads: Arc<Semaphore>,
    read: impl FnOnce(&Path, &AtomicU64) -> Result<Cover, TagEditorError> + Send + 'static,
) -> Result<TagEditorView, TagEditorError> {
    let permit = reads.try_acquire_owned().map_err(|_| TagEditorError::CoverBusy)?;
    let progress = Arc::new(DropProgress::default());
    let reading = Arc::clone(&progress);
    let mut watch = ProgressWatch {
        progress,
        seen: 0,
        moved: tokio::time::Instant::now(),
    };
    let cover = blocking_typed_result_until_stalled(
        stall_limit,
        &mut watch,
        |_| TagEditorError::TaskFailed,
        move || {
            // Held until the read itself ends, given up on or not.
            let _permit = permit;
            let result = read(&path, &reading.bytes);
            reading.done.store(true, Ordering::Release);
            result
        },
    )
    .await?;
    edit_off_thread(session_id, cover).await
}

/// Gives every readable file a pasted picture (its base64 text, decoded here, off the IPC
/// thread, and only when no longer than the largest paste taken), in memory. It becomes a cover within
/// `limit` (a TIFF is decoded and re-encoded, which grows with the picture); past it the answer is
/// `PasteTimedOut`, and the conversion finishing later changes nothing: as for a drop, the edit
/// is made only from the awaited answer.
pub async fn set_from_bytes_within(
    session_id: String,
    data: String,
    limit: Duration,
) -> Result<TagEditorView, TagEditorError> {
    let cover = blocking_typed_result_with_timeout(
        limit,
        || TagEditorError::PasteTimedOut,
        |_| TagEditorError::TaskFailed,
        move || cover_from_bytes(super::cover::decode_pasted(&data)?),
    )
    .await?;
    edit_off_thread(session_id, cover).await
}

async fn edit_off_thread(session_id: String, cover: Cover) -> Result<TagEditorView, TagEditorError> {
    tokio::task::spawn_blocking(move || set_for_all(&session_id, cover))
        .await
        .map_err(|_| TagEditorError::TaskFailed)?
}
