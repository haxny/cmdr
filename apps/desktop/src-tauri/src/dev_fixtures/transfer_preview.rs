//! The `transfer-progress` gallery row's tree: real folders the preview copies,
//! paced per file so the progress dialog's live file tree is there to review.
//!
//! Each top-level folder under `From/` is one of the tree's shapes (several
//! folders of photos, single-child chains that compact into one row, one flat
//! folder of many files). The shapes the catalog in Debug → Components covers
//! but a local copy can't stage honestly (a parallel SMB copy, failed rows) stay
//! there. `To/` is where every preview copies to, and it's emptied on every
//! trigger, so a copy never parks on a clash with the last one.
//!
//! The pace is `test_mode::pace_copies_from`: copies out of `From/` sleep after
//! each file, and nothing else in the app is slowed.

use std::fs;
use std::path::Path;

use super::{remove_entry, write_file};

/// The preview's own directory name under the app data dir.
pub const TRANSFER_FIXTURE_DIR_NAME: &str = "dialog-gallery-transfer-fixtures";

/// How long each file of a preview copy takes: long enough to read a row, short
/// enough that the folders shape finishes in under a minute.
const PACE_MS: u64 = 400;

/// Five folders of 20 photos: done folders above the work, waiting ones below.
const FOLDERS_SOURCE: &str = "Holiday photos";
/// Single-child chains (`src/lib/…`, `assets/Photos/2024`) that share one row.
const COMPACT_SOURCE: &str = "Project archive";
/// One folder of many files, for the long flat list.
const FLAT_SOURCE: &str = "Scanned receipts";

/// What the compact-chains shape holds, relative to its folder.
const COMPACT_FILES: &[(&str, u64)] = &[
    ("README.md", 6_144),
    ("src/lib/file-operations/transfer-tree/TransferTree.svelte", 24_576),
    ("src/lib/file-operations/transfer-tree/TransferTreeRow.svelte", 8_192),
    ("src/lib/file-operations/transfer-tree/row-format.ts", 12_288),
    ("src/lib/file-operations/transfer-tree/pinned-headers.ts", 5_120),
    ("assets/Photos/2024/IMG_0412.jpg", 3_145_728),
    ("assets/Photos/2024/IMG_0413.jpg", 2_883_584),
    ("assets/Photos/2024/IMG_0414.jpg", 3_407_872),
    ("assets/Photos/2024/IMG_0415.jpg", 2_621_440),
];

/// Where the `transfer-progress` preview copies from and to, and the folder each
/// state copies.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreviewFixtures {
    /// Absolute path of the folder holding every shape.
    pub from_dir: String,
    /// Absolute path of the folder every preview copies into, empty on each trigger.
    pub to_dir: String,
    /// Name of the five-folders shape inside `from_dir`.
    pub folders: String,
    /// Name of the compact-chains shape inside `from_dir`.
    pub compact_chains: String,
    /// Name of the flat shape inside `from_dir`.
    pub flat: String,
}

/// Creates (or completes) `From/`, empties `To/`, and paces copies out of `From/`.
pub(super) fn ensure_transfer_preview_fixtures(root: &Path) -> Result<TransferPreviewFixtures, String> {
    let from = root.join("From");
    let to = root.join("To");

    for (relative, size) in shape_files() {
        let path = from.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        write_file(&path, size)?;
    }

    fs::create_dir_all(&to).map_err(|e| format!("Failed to create {}: {e}", to.display()))?;
    for entry in fs::read_dir(&to).map_err(|e| format!("Failed to read {}: {e}", to.display()))? {
        let path = entry
            .map_err(|e| format!("Failed to read {}: {e}", to.display()))?
            .path();
        remove_entry(&path)?;
    }

    crate::test_mode::pace_copies_from(from.clone(), PACE_MS);

    Ok(TransferPreviewFixtures {
        from_dir: from.to_string_lossy().into_owned(),
        to_dir: to.to_string_lossy().into_owned(),
        folders: FOLDERS_SOURCE.to_string(),
        compact_chains: COMPACT_SOURCE.to_string(),
        flat: FLAT_SOURCE.to_string(),
    })
}

/// Every file of every shape, relative to `From/`, with its length. Sizes vary
/// so the rows' byte counts differ; files are sparse (`write_file`), so the
/// whole tree costs a few hundred kilobytes of real disk.
fn shape_files() -> Vec<(String, u64)> {
    let mut files = Vec::new();
    for day in 1..=5_u64 {
        for photo in 1..=20_u64 {
            let size = 1_800_000 + (day * 20 + photo) * 37_123 % 2_000_000;
            files.push((
                format!("{FOLDERS_SOURCE}/Day {day}/IMG_{:04}.jpg", day * 100 + photo),
                size,
            ));
        }
    }
    for (relative, size) in COMPACT_FILES {
        files.push((format!("{COMPACT_SOURCE}/{relative}"), *size));
    }
    for receipt in 1..=300_u64 {
        files.push((
            format!("{FLAT_SOURCE}/receipt-{receipt:03}.pdf"),
            40_000 + receipt * 211,
        ));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("readable")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_shape_is_on_disk() {
        let temp = tempfile::tempdir().expect("temp dir");
        let fixtures = ensure_transfer_preview_fixtures(temp.path()).expect("first run should succeed");
        let from = Path::new(&fixtures.from_dir);

        assert_eq!(names_in(&from.join(&fixtures.folders)).len(), 5);
        assert_eq!(names_in(&from.join(&fixtures.folders).join("Day 3")).len(), 20);
        assert!(
            from.join(&fixtures.compact_chains)
                .join("src/lib/file-operations/transfer-tree/row-format.ts")
                .is_file()
        );
        assert_eq!(names_in(&from.join(&fixtures.flat)).len(), 300);
        assert!(names_in(Path::new(&fixtures.to_dir)).is_empty());
    }

    /// The last preview's copy really landed in `To/`; the next trigger clears it
    /// and leaves `From/` alone.
    #[test]
    fn each_trigger_empties_the_destination_only() {
        let temp = tempfile::tempdir().expect("temp dir");
        let fixtures = ensure_transfer_preview_fixtures(temp.path()).expect("first run should succeed");
        let to = Path::new(&fixtures.to_dir);
        fs::create_dir_all(to.join("Holiday photos/Day 1")).expect("a landed folder");
        fs::write(to.join("Holiday photos/Day 1/IMG_0101.jpg"), b"copied").expect("a landed file");
        let reviewer_file = Path::new(&fixtures.from_dir).join(&fixtures.flat).join("reviewer.txt");
        fs::write(&reviewer_file, b"kept").expect("a reviewer's file");

        ensure_transfer_preview_fixtures(temp.path()).expect("second run should succeed");

        assert!(names_in(to).is_empty(), "the destination still holds the last copy");
        assert!(reviewer_file.is_file(), "the sources lost a file");
    }
}
