//! Cover images from the session's folder: which files can be one, and reading one.
//!
//! A cover is named by a plain file name and resolved strictly inside the session's folder:
//! no separators, no `..`, and a symlink that leads out of the folder is refused too. Only a
//! regular file is read, and its type comes from its first bytes (JPEG, PNG, WebP), never its
//! name.

use std::io::Read;
use std::path::{Component, Path};

use cmdr_tags::Cover;
use serde::Serialize;

use super::session::TagEditorError;

/// The largest cover file read.
pub(crate) const MAX_COVER_BYTES: u64 = 10 * 1024 * 1024;

/// An image file of the session's folder that can become the cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoverCandidate {
    pub file_name: String,
    pub size: u64,
}

/// The MIME type a supported image extension suggests. It only picks the candidates; what a
/// cover is comes from its bytes ([`image_mime`]).
fn mime_of(name: &str) -> Option<&'static str> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// The jpg, jpeg, png and webp files directly in `dir`, by name. Blocking file work.
pub(crate) fn candidates_in(dir: &Path) -> Result<Vec<CoverCandidate>, TagEditorError> {
    let entries = std::fs::read_dir(dir).map_err(|_| TagEditorError::CoverUnreadable)?;
    let mut found: Vec<CoverCandidate> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            mime_of(&file_name)?;
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some(CoverCandidate {
                file_name,
                size: meta.len(),
            })
        })
        .collect();
    found.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(found)
}

/// Whether `name` is exactly one plain path component.
fn is_plain_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\', '\0']) {
        return false;
    }
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// The image type of `data`, from its first bytes: JPEG, PNG, or WebP. The name says nothing
/// (a WebP saved as `.jpg` is common with web cover art; an HTML error page can be too).
pub(crate) fn image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"\x89PNG\r\n\x1A\n") {
        Some("image/png")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The cover as a `data:` URL for the thumbnail (the app's CSP allows `data:` images, not
/// `blob:`), typed by its bytes; `None` when they aren't a JPEG, PNG, or WebP image.
pub(crate) fn thumbnail_url(data: &[u8]) -> Option<String> {
    use base64::Engine;
    let mime = image_mime(data)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::prelude::BASE64_STANDARD.encode(data)
    ))
}

/// Reads the image `file_name` of `dir` as a cover, its MIME type taken from its bytes. Blocking
/// file work.
///
/// Only a regular file is opened (a FIFO named `x.jpg` would block a pool thread forever), and
/// it is opened at its resolved path with `O_NOFOLLOW | O_NONBLOCK`, so a symlink or FIFO
/// swapped in after the checks fails instead of leading out of the folder or blocking. A symlink
/// whose target sits directly in the folder reads that target.
pub(crate) fn read_cover(dir: &Path, file_name: &str) -> Result<Cover, TagEditorError> {
    if !is_plain_name(file_name) {
        return Err(TagEditorError::BadCoverName);
    }
    mime_of(file_name).ok_or(TagEditorError::CoverUnreadable)?;
    let path = dir.join(file_name);
    // Resolved, the file must still sit directly in the folder (a symlink out is refused).
    let resolved = path.canonicalize().map_err(|_| TagEditorError::CoverUnreadable)?;
    let folder = dir.canonicalize().map_err(|_| TagEditorError::CoverUnreadable)?;
    if resolved.parent() != Some(folder.as_path()) {
        return Err(TagEditorError::BadCoverName);
    }
    let regular = std::fs::symlink_metadata(&resolved).is_ok_and(|meta| meta.file_type().is_file());
    if !regular {
        return Err(TagEditorError::CoverUnreadable);
    }
    let file = open_no_follow(&resolved).map_err(|_| TagEditorError::CoverUnreadable)?;
    let meta = file.metadata().map_err(|_| TagEditorError::CoverUnreadable)?;
    if !meta.is_file() {
        return Err(TagEditorError::CoverUnreadable);
    }
    if meta.len() > MAX_COVER_BYTES {
        return Err(TagEditorError::CoverTooLarge);
    }
    let data = read_capped(file, MAX_COVER_BYTES)?;
    let mime = image_mime(&data).ok_or(TagEditorError::CoverNotAnImage)?;
    Ok(Cover {
        mime: mime.to_owned(),
        data: data.into(),
    })
}

/// Opens `path` for reading without following a symlink at its last component and without
/// blocking on a FIFO.
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

/// Reads at most `limit` bytes; more is `CoverTooLarge` (the file grew since the stat).
pub(crate) fn read_capped(file: impl Read, limit: u64) -> Result<Vec<u8>, TagEditorError> {
    let mut data = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| TagEditorError::CoverUnreadable)?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > limit {
        return Err(TagEditorError::CoverTooLarge);
    }
    Ok(data)
}
