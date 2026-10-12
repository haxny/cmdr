//! Tests for the listing operations: cancel and the volume-backed start.

// ============================================================================
// Tests for streaming directory listing
// ============================================================================

#[test]
fn test_cancel_listing_sets_flag() {
    use super::cancel_listing;
    use super::streaming::{StreamingListingState, streaming_state};
    use std::sync::Arc;

    // Create a test listing ID and state
    let listing_id = "test-cancel-listing-12345";
    let state = Arc::new(StreamingListingState::new());

    // Store it in the streaming state cache
    {
        let mut cache = streaming_state().write().unwrap();
        cache.insert(listing_id.to_string(), Arc::clone(&state));
    }

    // Verify the token starts un-cancelled
    assert!(!state.cancel.is_cancelled());

    // Call cancel_listing
    cancel_listing(listing_id);

    // Verify the token is now cancelled
    assert!(state.cancel.is_cancelled());

    // Cleanup
    {
        let mut cache = streaming_state().write().unwrap();
        cache.remove(listing_id);
    }
}

#[test]
fn test_cancel_listing_nonexistent_does_not_panic() {
    use super::cancel_listing;

    // Should not panic when listing doesn't exist
    cancel_listing("nonexistent-listing-id");
}

// ============================================================================
// Tests for list_directory_start_with_volume
// ============================================================================

/// Tests that `list_directory_start_with_volume` reads entries from an InMemoryVolume,
/// caches them in the listing cache, and returns the correct count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_caches_entries() {
    use crate::file_system::listing::caching_test_support::TestListingGuard;
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_fs::entry::FileEntry;
    use cmdr_fs::volume::InMemoryVolume;
    use std::path::PathBuf;
    use std::sync::Arc;

    let volume_id = format!("test-vol-ldswv-{}", uuid::Uuid::new_v4());
    let dir_path = PathBuf::from("/testdir");

    // Create volume with three files
    let volume = Arc::new(InMemoryVolume::with_entries(
        "TestLDSWV",
        vec![
            FileEntry {
                size: Some(100),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("p.txt".to_string(), "/testdir/p.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(200),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("q.txt".to_string(), "/testdir/q.txt".to_string(), false, false)
            },
            FileEntry {
                size: Some(300),
                permissions: 0o644,
                owner: "test".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("r.txt".to_string(), "/testdir/r.txt".to_string(), false, false)
            },
        ],
    ));

    get_volume_manager().register(&volume_id, volume);

    let result = super::operations::list_directory_start_with_volume(
        &volume_id,
        &dir_path,
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(
        result.is_ok(),
        "list_directory_start_with_volume failed: {:?}",
        result.err()
    );
    let result = result.unwrap();
    assert_eq!(result.total_count, 3);

    // The production call created the cache entry and its watcher; hand both to a
    // guard so they're torn down even if an assertion below panics.
    let listing = TestListingGuard::adopt(&result.listing_id);

    let names = listing.entry_names();
    assert_eq!(names.len(), 3);
    assert!(names.iter().any(|n| n == "p.txt"));
    assert!(names.iter().any(|n| n == "q.txt"));
    assert!(names.iter().any(|n| n == "r.txt"));
    listing.with_listing(|cached| {
        assert_eq!(cached.volume_id, volume_id);
        assert_eq!(cached.path.as_path(), std::path::Path::new(&dir_path));
    });

    get_volume_manager().unregister(&volume_id);
}

/// Tests that `list_directory_start_with_volume` returns an error for a nonexistent volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_list_directory_start_with_volume_unknown_volume() {
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use std::path::PathBuf;

    let result = super::operations::list_directory_start_with_volume(
        "nonexistent-volume-id",
        &PathBuf::from("/some/path"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
