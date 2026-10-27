//! Cover images from the session's folder: which files can be one, and reading one.
//!
//! A cover is named by a plain file name and resolved strictly inside the session's folder:
//! no separators, no `..`, and a symlink that leads out of the folder is refused too.

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

/// The MIME type for a supported image extension.
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

/// Reads the image `file_name` of `dir` as a cover. Blocking file work.
pub(crate) fn read_cover(dir: &Path, file_name: &str) -> Result<Cover, TagEditorError> {
    if !is_plain_name(file_name) {
        return Err(TagEditorError::BadCoverName);
    }
    let mime = mime_of(file_name).ok_or(TagEditorError::CoverUnreadable)?;
    let path = dir.join(file_name);
    // Resolved, the file must still sit directly in the folder (a symlink out is refused).
    let resolved = path.canonicalize().map_err(|_| TagEditorError::CoverUnreadable)?;
    let folder = dir.canonicalize().map_err(|_| TagEditorError::CoverUnreadable)?;
    if resolved.parent() != Some(folder.as_path()) {
        return Err(TagEditorError::BadCoverName);
    }
    let file = std::fs::File::open(&resolved).map_err(|_| TagEditorError::CoverUnreadable)?;
    let meta = file.metadata().map_err(|_| TagEditorError::CoverUnreadable)?;
    if !meta.is_file() {
        return Err(TagEditorError::CoverUnreadable);
    }
    if meta.len() > MAX_COVER_BYTES {
        return Err(TagEditorError::CoverTooLarge);
    }
    // The limit is enforced on the read too, in case the file grew since the stat.
    let mut data = Vec::new();
    file.take(MAX_COVER_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| TagEditorError::CoverUnreadable)?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > MAX_COVER_BYTES {
        return Err(TagEditorError::CoverTooLarge);
    }
    Ok(Cover {
        mime: mime.to_owned(),
        data: data.into(),
    })
}
