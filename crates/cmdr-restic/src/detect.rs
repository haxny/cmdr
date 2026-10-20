//! Recognizing a restic repository from its folder's own listing.

use cmdr_fs::entry::FileEntry;

/// Whether a folder whose listing is `entries` is a restic repository: a regular
/// file named `config` plus the `data`, `index`, `keys`, and `snapshots` folders.
///
/// `locks` may be missing (restic creates it lazily, and a copied repo often
/// drops it). A listing of more than [`MAX_REPO_ROOT_ENTRIES`] isn't one, on
/// its length alone. Pure and O(1) for a big folder: it reads the listing a pane
/// already has and never touches the disk, so a caller can ask it per listing. ❌ It never tries the
/// password: without it the config is unreadable, so decryption can't detect.
pub fn is_restic_repo(entries: &[FileEntry]) -> bool {
    if entries.len() > MAX_REPO_ROOT_ENTRIES {
        return false;
    }
    let has = |name: &str, is_directory: bool| {
        entries
            .iter()
            .any(|e| e.name == name && e.is_directory == is_directory && !e.is_symlink)
    };
    has("config", false) && REQUIRED_DIRS.iter().all(|name| has(name, true))
}

/// The most entries a repo's folder may hold and still count as one. A repo
/// root holds six; a crowded folder that happens to hold the five names isn't
/// a repo anyone browses, and answering on the length keeps a 100k-entry
/// listing from being scanned.
pub(crate) const MAX_REPO_ROOT_ENTRIES: usize = 32;

/// The folders every repo has from `restic init` on. `locks` is deliberately not
/// one of them.
const REQUIRED_DIRS: [&str; 4] = ["data", "index", "keys", "snapshots"];

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
