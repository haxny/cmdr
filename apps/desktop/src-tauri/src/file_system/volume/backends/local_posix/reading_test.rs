//! Tests for the local reader: `std::fs` listing, single-entry stat, and `FileEntry` construction.

use super::reading::{get_extended_metadata_batch, get_single_entry, list_directory_core, process_dir_entry};
use crate::file_system::provider::FileSystemProvider;
use crate::file_system::real_provider::RealFileSystemProvider;
use crate::test_support::TestDir;
use cmdr_fs::volume::host::VolumeHost;
use std::fs;

#[test]
fn test_list_directory() {
    let provider = RealFileSystemProvider;
    // Create our own temp directory to avoid permission issues
    let temp_dir = TestDir::new("list_test");

    let result = provider.list_directory(&temp_dir);

    assert!(result.is_ok(), "list_directory failed: {:?}", result.err());
}

#[test]
fn test_list_directory_entries_have_names() {
    let provider = RealFileSystemProvider;
    let temp_dir = TestDir::new("ops_test");

    let test_file = temp_dir.join("test_file.txt");
    fs::write(&test_file, "content").unwrap();

    let entries = provider.list_directory(&temp_dir).unwrap();

    assert!(!entries.is_empty());
    assert!(entries.iter().any(|e| e.name == "test_file.txt"));
}

// ============================================================================
// Tests for two-phase loading functions
// ============================================================================

#[test]
fn test_list_directory_core_returns_entries_without_extended_metadata() {
    let temp_dir = TestDir::new("core_test");

    let test_file = temp_dir.join("core_test.txt");
    fs::write(&test_file, "content").unwrap();

    let entries = list_directory_core(&temp_dir, VolumeHost::detached().listings()).unwrap();

    assert!(!entries.is_empty());
    let file_entry = entries.iter().find(|e| e.name == "core_test.txt").unwrap();

    // Core metadata should be present
    assert!(!file_entry.name.is_empty());
    assert!(!file_entry.path.is_empty());
    assert!(!file_entry.owner.is_empty());

    // Extended metadata should NOT be loaded
    assert!(!file_entry.extended_metadata_loaded);
    assert!(file_entry.added_at.is_none());
    assert!(file_entry.opened_at.is_none());
}

#[test]
fn test_list_directory_core_is_sorted() {
    let temp_dir = TestDir::new("sort_test");

    // Create files in non-alphabetical order
    fs::write(temp_dir.join("zebra.txt"), "").unwrap();
    fs::write(temp_dir.join("alpha.txt"), "").unwrap();
    fs::create_dir(temp_dir.join("a_dir")).unwrap();

    let entries = list_directory_core(&temp_dir, VolumeHost::detached().listings()).unwrap();

    // Directories should come first, then sorted alphabetically. The fixture dir is
    // ours alone, so the count is exact rather than a floor over someone's leftovers.
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "a_dir");
    assert!(entries[0].is_directory);
}

#[test]
fn test_get_extended_metadata_batch() {
    let temp_dir = TestDir::new("extended_test");

    let test_file = temp_dir.join("extended_test.txt");
    fs::write(&test_file, "content").unwrap();

    let paths = vec![test_file.to_string_lossy().to_string()];
    let extended = get_extended_metadata_batch(paths.clone());

    assert_eq!(extended.len(), 1);
    assert_eq!(extended[0].path, paths[0]);

    // On macOS, these should have values; on other platforms, None
    #[cfg(target_os = "macos")]
    {
        // addedAt and openedAt may or may not be set depending on the file
        // but the function should run without error
    }
}

#[test]
fn test_get_extended_metadata_batch_empty_input() {
    let extended = get_extended_metadata_batch(vec![]);
    assert!(extended.is_empty());
}

// ============================================================================
// Tests for get_single_entry
// ============================================================================

#[test]
fn test_get_single_entry_file() {
    let temp_dir = TestDir::new("single_entry_test");

    let test_file = temp_dir.join("single_file.txt");
    fs::write(&test_file, "test content").unwrap();

    let entry = get_single_entry(&test_file).unwrap();

    assert_eq!(entry.name, "single_file.txt");
    assert!(!entry.is_directory);
    assert!(!entry.is_symlink);
    assert_eq!(entry.size, Some(12)); // "test content" is 12 bytes
    assert!(!entry.extended_metadata_loaded);
}

#[test]
fn test_get_single_entry_directory() {
    let temp_dir = TestDir::new("single_dir_test");

    let entry = get_single_entry(&temp_dir).unwrap();

    assert!(entry.name.contains("cmdr_single_dir_test"));
    assert!(entry.is_directory);
    assert!(!entry.is_symlink);
    assert!(entry.size.is_none());
}

#[test]
fn test_get_single_entry_nonexistent() {
    let result = get_single_entry(std::path::Path::new("/definitely_does_not_exist_12345"));
    assert!(result.is_err());
}

#[test]
fn test_process_dir_entry_returns_file_entry() {
    let temp_dir = TestDir::new("process_entry_test");

    let test_file = temp_dir.join("process_test.txt");
    fs::write(&test_file, "test content").unwrap();

    // Read directory and get a DirEntry
    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == "process_test.txt").unwrap();

    let file_entry = process_dir_entry(dir_entry, &std::collections::HashSet::new());

    assert!(file_entry.is_some());
    let entry = file_entry.unwrap();
    assert_eq!(entry.name, "process_test.txt");
    assert!(!entry.is_directory);
    assert!(!entry.is_symlink);
    assert_eq!(entry.size, Some(12)); // "test content" is 12 bytes
}

#[test]
fn test_process_dir_entry_handles_directory() {
    let temp_dir = TestDir::new("process_dir_test");

    let sub_dir = temp_dir.join("sub_directory");
    fs::create_dir(&sub_dir).unwrap();

    // Read directory and get a DirEntry
    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == "sub_directory").unwrap();

    let file_entry = process_dir_entry(dir_entry, &std::collections::HashSet::new());

    assert!(file_entry.is_some());
    let entry = file_entry.unwrap();
    assert_eq!(entry.name, "sub_directory");
    assert!(entry.is_directory);
    assert!(entry.size.is_none());
}

// ============================================================================
// Tests for `is_hidden`: name, and (macOS) `UF_HIDDEN`
// ============================================================================

#[test]
fn test_process_dir_entry_marks_a_dotfile_hidden() {
    let temp_dir = TestDir::new("dotfile_hidden_test");
    fs::write(temp_dir.join(".secret"), "x").unwrap();

    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == ".secret").unwrap();

    let file_entry = process_dir_entry(dir_entry, &std::collections::HashSet::new()).expect("entry reads");
    assert!(file_entry.is_hidden, "a dotfile must be marked hidden");
}

#[test]
fn test_process_dir_entry_leaves_an_ordinary_file_visible() {
    let temp_dir = TestDir::new("ordinary_not_hidden_test");
    fs::write(temp_dir.join("visible.txt"), "x").unwrap();

    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries.iter().find(|e| e.file_name() == "visible.txt").unwrap();

    let file_entry = process_dir_entry(dir_entry, &std::collections::HashSet::new()).expect("entry reads");
    assert!(!file_entry.is_hidden, "an ordinary file must not be marked hidden");
}

/// `chflags hidden` sets `UF_HIDDEN` in `st_flags`, and macOS honors it with no
/// dot in the name at all. `LocalPosixVolume` must too.
#[cfg(target_os = "macos")]
#[test]
fn test_process_dir_entry_honors_uf_hidden_with_no_dot_in_the_name() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let temp_dir = TestDir::new("uf_hidden_test");
    let file_path = temp_dir.join("no_dot_but_hidden.txt");
    fs::write(&file_path, "x").unwrap();

    let c_path = CString::new(file_path.as_os_str().as_bytes()).expect("path has no interior NUL");
    // SAFETY: `c_path` is a live, NUL-terminated C string naming a regular file
    // this test just created and owns for its whole body; `chflags` only writes
    // that file's on-disk flags, nothing is read back through the pointer.
    let result = unsafe { libc::chflags(c_path.as_ptr(), libc::UF_HIDDEN) };
    assert_eq!(
        result, 0,
        "chflags(UF_HIDDEN) must succeed on a file this test just created"
    );

    let entries: Vec<_> = fs::read_dir(&temp_dir).unwrap().filter_map(|e| e.ok()).collect();
    let dir_entry = entries
        .iter()
        .find(|e| e.file_name() == "no_dot_but_hidden.txt")
        .unwrap();

    let file_entry = process_dir_entry(dir_entry, &std::collections::HashSet::new()).expect("entry reads");
    assert!(
        file_entry.is_hidden,
        "UF_HIDDEN must mark an entry hidden even though its name carries no dot"
    );
}
