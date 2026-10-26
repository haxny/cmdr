//! The safe overwrite every write goes through: work on a clone of the file beside it, and
//! only a finished, checked clone replaces the original.

use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Seek};
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use tempfile::NamedTempFile;

use crate::model::TagError;

/// Clones the file at `path` beside it, lets `save` write the tags into the clone, fsyncs
/// it, and renames it over the original. Any failure, a panic in `save` included, leaves the
/// original as it was and removes the clone. The clone keeps the original's permissions (and,
/// on macOS, its extended attributes such as Finder tags); a symlink is followed, so the link
/// stays a link. A read-only file is refused.
pub(crate) fn save_safely(path: &Path, save: impl FnOnce(&mut File) -> Result<(), TagError>) -> Result<(), TagError> {
    let target = fs::canonicalize(path)?;
    let dir = target
        .parent()
        .ok_or_else(|| TagError::Write("the file has no parent directory".to_owned()))?;
    let permissions = fs::metadata(&target)?.permissions();
    if permissions.readonly() {
        return Err(TagError::Io(
            ErrorKind::PermissionDenied,
            "the file is read-only".to_owned(),
        ));
    }
    let mut temp = clone_beside(&target, dir)?;
    let file = temp.as_file_mut();
    // lofty has panicked on input it can't encode; a panic must fail this write, not the app.
    panic::catch_unwind(AssertUnwindSafe(|| save(file)))
        .unwrap_or_else(|_| Err(TagError::Write("the tag library panicked while saving".to_owned())))?;
    file.set_permissions(permissions)?;
    file.sync_all()?;
    temp.persist(&target).map_err(|e| TagError::from(e.error))?;
    // The rename is durable only once the directory is synced. The new file is in place
    // either way, so a failure here doesn't fail the write.
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        log::warn!(target: "cmdr_tags::save", "Tags written, but syncing the folder after the rename didn't work: {e}");
    }
    Ok(())
}

/// A clone of `target` at a fresh temp path in `dir`, open for reading and writing, removed
/// when dropped unless persisted. The path must not exist yet: on APFS `fs::copy` then clones
/// the file (`fclonefileat`), so a GB-size audiobook costs no copy; elsewhere it copies. Both
/// carry permissions and, on macOS, extended attributes.
fn clone_beside(target: &Path, dir: &Path) -> Result<NamedTempFile<File>, TagError> {
    let temp = tempfile::Builder::new()
        .prefix(".cmdr-tags-")
        .suffix(".tmp")
        .make_in(dir, |path| {
            if path.try_exists()? {
                // `make_in` retries with another name.
                return Err(io::Error::from(ErrorKind::AlreadyExists));
            }
            let opened = fs::copy(target, path)
                .and_then(|_| OpenOptions::new().read(true).write(true).open(path))
                .and_then(|mut file| file.rewind().map(|()| file));
            if opened.is_err() {
                // The copy may have created the file before failing; the copy's error is the one
                // reported, so a failed clean-up here (usually: nothing to remove) adds nothing.
                fs::remove_file(path).ok();
            }
            opened
        })?;
    Ok(temp)
}
