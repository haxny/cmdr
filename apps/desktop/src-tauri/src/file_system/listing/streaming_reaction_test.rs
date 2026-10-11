//! The terminal-reaction matrix of one streaming listing: for every way a pane's
//! read can end, which events it emits in which order, and which app bookkeeping
//! runs around it (the MCP recent-errors ring, the analytics category, the
//! TCC-restricted set's record and clear, and the stale-mount check).
//!
//! Each cell drives exactly what `list_directory_start_streaming`'s spawned task
//! does, minus the `AppHandle`: `read_directory_with_progress`, then
//! `finish_streaming_listing` with a recording [`ListingEndEffects`]. Issue #301
//! moves this policy behind the `ListingEvents` seam, and these cells are the
//! proof that the move kept every reaction.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::caching_test_support::{SpelledVolume, TestListingGuard, unique_test_id};
use super::listing_end::{ListingEndEffects, error_analytics_category, finish_streaming_listing};
use super::seam_test_support::cancel_at_decorate;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use super::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::{TestDir, wait_until_async};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::friendly_error::{ListingError, ListingErrorReason, Provider};
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};

/// One bookkeeping call, as `finish_streaming_listing` made it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    /// The MCP ring got an entry, with this typed reason.
    RecentError(String),
    RecordDenial(PathBuf),
    ClearRestricted(PathBuf),
    NoteRootFailure(String),
}

#[derive(Default)]
struct RecordingEffects {
    calls: std::sync::Mutex<Vec<Effect>>,
}

impl RecordingEffects {
    fn calls(&self) -> Vec<Effect> {
        self.calls.lock_ignore_poison().clone()
    }
}

impl ListingEndEffects for RecordingEffects {
    fn record_recent_error(
        &self,
        _listing_id: &str,
        _volume_id: &str,
        _path: &Path,
        _message: &str,
        error: &ListingError,
    ) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::RecentError(format!("{:?}", error.reason)));
    }

    fn record_denial(&self, path: &Path) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::RecordDenial(path.to_path_buf()));
    }

    fn clear_restricted(&self, path: &Path) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::ClearRestricted(path.to_path_buf()));
    }

    fn note_root_failure(&self, volume_id: &str, _error: &VolumeError) {
        self.calls
            .lock_ignore_poison()
            .push(Effect::NoteRootFailure(volume_id.to_string()));
    }
}

/// What one listing did, end to end.
struct Ended {
    events: Vec<&'static str>,
    effects: Vec<Effect>,
    /// The typed classification of each `listing-error`, in order.
    errors: Vec<Option<ListingError>>,
    /// The analytics category each `listing-error` reports.
    analytics: Vec<Option<String>>,
    listing: TestListingGuard,
}

/// Runs one streaming listing of `path` on `volume_id` through the spawned
/// task's two steps, with `prepare` given the read's state before it starts.
async fn run_listing(volume_id: &str, path: &Path, state: Arc<StreamingListingState>) -> Ended {
    let listing = TestListingGuard::adopt(unique_test_id("reaction"));
    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let result = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        volume_id,
        path,
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;
    let effects = RecordingEffects::default();
    finish_streaming_listing(events.as_ref(), &effects, listing.id(), volume_id, path, result);
    let errors: Vec<Option<ListingError>> = sink
        .classified
        .lock_ignore_poison()
        .iter()
        .map(|(_, e)| e.clone())
        .collect();
    let analytics = errors.iter().map(|e| error_analytics_category(e.as_ref())).collect();
    Ended {
        events: sink.sequence(),
        effects: effects.calls(),
        errors,
        analytics,
        listing,
    }
}

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/{name}"), false, false)
}

/// Registers `volume` under a fresh id; the guard unregisters it.
struct Registered(String);

impl Registered {
    fn new(tag: &str, volume: Arc<dyn Volume>) -> Self {
        let id = unique_test_id(tag);
        get_volume_manager().register(&id, volume);
        Self(id)
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.0);
    }
}

fn refused_with(tag: &str, error: VolumeError) -> Registered {
    Registered::new(
        tag,
        Arc::new(SpelledVolume::new(InMemoryVolume::new("Refusing")).listings_refused_with(error)),
    )
}

/// A path inside a Dropbox folder, which provider detection reads lexically.
fn cloud_path(name: &str) -> PathBuf {
    dirs::home_dir()
        .expect("a test runner has a home directory")
        .join("Library/CloudStorage/Dropbox-seam-pins")
        .join(name)
}

fn reason_of(ended: &Ended) -> String {
    let [Some(error)] = ended.errors.as_slice() else {
        panic!("expected one classified error, got {:?}", ended.errors);
    };
    format!("{:?}", error.reason)
}

// ============================================================================
// Success
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_that_lands_completes_and_clears_its_path_from_the_restricted_set() {
    let volume = Registered::new(
        "reaction-ok",
        Arc::new(InMemoryVolume::with_entries("Ok", vec![entry("a.txt")])),
    );
    let ended = run_listing(&volume.0, Path::new("/"), Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "read_complete", "complete"]);
    assert_eq!(ended.effects, [Effect::ClearRestricted(PathBuf::from("/"))]);
    assert!(ended.analytics.is_empty(), "no error, no analytics");
    assert!(ended.listing.is_cached());
}

// ============================================================================
// Cancel, at each of its four points
// ============================================================================

/// Every cancel ends `Ok`, so the task clears the path from the restricted set
/// exactly as a success does, and nothing reaches the error bookkeeping.
fn assert_cancelled(ended: &Ended, events: &[&str], path: &Path) {
    assert_eq!(ended.events, events);
    assert_eq!(ended.effects, [Effect::ClearRestricted(path.to_path_buf())]);
    assert!(ended.analytics.is_empty());
    assert!(!ended.listing.is_cached(), "a cancelled read never lands in the cache");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_before_the_read_ends_the_listing_unread() {
    let volume = Registered::new(
        "reaction-cancel-before",
        Arc::new(InMemoryVolume::with_entries("Before", vec![entry("a.txt")])),
    );
    let state = Arc::new(StreamingListingState::new());
    state.cancel.cancel();

    let ended = run_listing(&volume.0, Path::new("/"), state).await;

    assert_cancelled(&ended, &["opening", "cancelled"], Path::new("/"));
}

/// A volume whose read waits until the listing is cancelled, and says when it
/// started.
struct WaitsForCancel {
    root: PathBuf,
    started: Arc<AtomicBool>,
}

impl Volume for WaitsForCancel {
    fn name(&self) -> &str {
        "Waits for cancel"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let cancel = cancel.cloned().expect("a pane listing hands its read the cancel token");
        let started = Arc::clone(&self.started);
        Box::pin(async move {
            started.store(true, Ordering::SeqCst);
            // Bounded, so a lost cancel fails the test instead of hanging the suite.
            let _ = tokio::time::timeout(Duration::from_secs(30), cancel.cancelled()).await;
            Ok(Vec::new())
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(true) })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_read_ends_the_listing_without_waiting_for_it() {
    let started = Arc::new(AtomicBool::new(false));
    let volume = Registered::new(
        "reaction-cancel-during",
        Arc::new(WaitsForCancel {
            root: PathBuf::from("/"),
            started: Arc::clone(&started),
        }),
    );
    let state = Arc::new(StreamingListingState::new());

    let run = {
        let volume_id = volume.0.clone();
        let state = Arc::clone(&state);
        tokio::spawn(async move { run_listing(&volume_id, Path::new("/"), state).await })
    };
    wait_until_async(Duration::from_secs(10), "the read to start", || {
        started.load(Ordering::SeqCst)
    })
    .await;
    state.cancel.cancel();
    let ended = run.await.expect("the listing task doesn't panic");

    assert_cancelled(&ended, &["opening", "cancelled"], Path::new("/"));
}

/// Cancels the listing the moment the read reports complete: after the read,
/// before the after-read check. Delegates every event to the collector.
struct CancelOnReadComplete {
    inner: Arc<CollectorListingEventSink>,
    cancel: CancellationToken,
}

impl ListingEventSink for CancelOnReadComplete {
    fn emit_opening(&self, listing_id: &str) {
        self.inner.emit_opening(listing_id);
    }
    fn emit_stalled(&self, listing_id: &str, stalled_on: super::stalled_on::StalledOn) {
        self.inner.emit_stalled(listing_id, stalled_on);
    }
    fn emit_progress(&self, listing_id: &str, loaded_count: usize) {
        self.inner.emit_progress(listing_id, loaded_count);
    }
    fn emit_read_complete(&self, listing_id: &str, total_count: usize) {
        self.inner.emit_read_complete(listing_id, total_count);
        self.cancel.cancel();
    }
    fn emit_complete(&self, listing_id: &str, total_count: usize, volume_root: String, stored_path: Option<String>) {
        self.inner
            .emit_complete(listing_id, total_count, volume_root, stored_path);
    }
    fn emit_error(&self, listing_id: &str, message: String, error: Option<ListingError>) {
        self.inner.emit_error(listing_id, message, error);
    }
    fn emit_cancelled(&self, listing_id: &str) {
        self.inner.emit_cancelled(listing_id);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_after_the_read_ends_the_listing_before_it_is_cached() {
    let volume = Registered::new(
        "reaction-cancel-after",
        Arc::new(InMemoryVolume::with_entries("After", vec![entry("a.txt")])),
    );
    let state = Arc::new(StreamingListingState::new());
    let listing = TestListingGuard::adopt(unique_test_id("reaction-after"));
    let collector = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::new(CancelOnReadComplete {
        inner: Arc::clone(&collector),
        cancel: state.cancel.clone(),
    });

    let result = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &volume.0,
        Path::new("/"),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;
    let effects = RecordingEffects::default();
    finish_streaming_listing(
        events.as_ref(),
        &effects,
        listing.id(),
        &volume.0,
        Path::new("/"),
        result,
    );

    assert_eq!(collector.sequence(), ["opening", "read_complete", "cancelled"]);
    assert_eq!(effects.calls(), [Effect::ClearRestricted(PathBuf::from("/"))]);
    assert!(!listing.is_cached(), "a cancelled read never lands in the cache");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_at_the_cache_insert_leaves_nothing_cached() {
    let volume_name = unique_test_id("reaction-cancel-insert-volume");
    let volume = Registered::new(
        "reaction-cancel-insert",
        Arc::new(InMemoryVolume::with_entries(volume_name.as_str(), vec![entry("a.txt")])),
    );
    let state = Arc::new(StreamingListingState::new());
    // Fires at the overlay step: past the after-read check, before the insert.
    cancel_at_decorate(&volume_name, state.cancel.clone());

    let ended = run_listing(&volume.0, Path::new("/"), state).await;

    assert_cancelled(&ended, &["opening", "read_complete", "cancelled"], Path::new("/"));
}

// ============================================================================
// Refusals and errors
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_permission_denied_listing_records_the_denial() {
    let path = Path::new("/denied");
    let volume = refused_with(
        "reaction-denied",
        VolumeError::PermissionDenied {
            path: "/denied".to_string(),
            raw_os_error: Some(libc::EACCES),
        },
    );

    let ended = run_listing(&volume.0, path, Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "error"]);
    let reason = reason_of(&ended);
    assert_eq!(
        ended.effects,
        [
            Effect::RecordDenial(path.to_path_buf()),
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason),
        ]
    );
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
}

/// Under a cloud provider's folder on purpose: a password prompt names no
/// provider, where a generic error there would.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_that_needs_a_password_asks_for_it() {
    let path = cloud_path("locked.zip");
    let volume = refused_with("reaction-password", VolumeError::NeedsPassword { wrong_attempt: false });

    let ended = run_listing(&volume.0, &path, Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::ArchiveNeedsPassword { wrong_attempt: false },
                provider: None,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_damaged_archive_reads_as_unreadable() {
    let scratch = TestDir::new("reaction-damaged-archive");
    let archive = scratch.join("broken.zip");
    std::fs::write(&archive, b"not a zip at all").expect("scratch dir is writable");
    let volume = refused_with("reaction-damaged", VolumeError::NotSupported);

    let ended = run_listing(&volume.0, &archive, Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::ArchiveUnreadable,
                provider: None,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("serious".to_string())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_generic_error_names_the_cloud_provider_behind_the_path() {
    let path = cloud_path("folder");
    let volume = refused_with(
        "reaction-provider",
        VolumeError::IoError {
            message: "Operation timed out".to_string(),
            raw_os_error: Some(libc::ETIMEDOUT),
        },
    );

    let ended = run_listing(&volume.0, &path, Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                provider: Some(Provider::Dropbox),
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(
        ended.effects,
        [
            Effect::NoteRootFailure(volume.0.clone()),
            Effect::RecentError(reason_of(&ended)),
        ]
    );
    assert_eq!(ended.analytics, [Some("transient".to_string())]);
}

// ============================================================================
// The restricted-empty root
// ============================================================================

/// iCloud Drive without Full Disk Access lists as an empty root. The read
/// itself turns that into a `listing-error` and records it in the MCP ring, then
/// returns `Ok`, so the task ALSO clears the path from the restricted set, the
/// same as a success.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_icloud_root_reads_as_restricted_yet_clears_the_path() {
    // The literal id `listing_error_for_restricted_empty_root` matches. No other
    // test registers it.
    let volume_id = "cloud-icloud";
    get_volume_manager().register(volume_id, Arc::new(InMemoryVolume::new("iCloud Drive")));
    struct Unregister;
    impl Drop for Unregister {
        fn drop(&mut self) {
            get_volume_manager().unregister("cloud-icloud");
        }
    }
    let _unregister = Unregister;

    let ended = run_listing(volume_id, Path::new("/"), Arc::new(StreamingListingState::new())).await;

    assert_eq!(ended.events, ["opening", "read_complete", "error"]);
    assert!(
        matches!(
            ended.errors.as_slice(),
            [Some(ListingError {
                reason: ListingErrorReason::EmptyRootICloud,
                ..
            })]
        ),
        "{:?}",
        ended.errors
    );
    assert_eq!(ended.effects, [Effect::ClearRestricted(PathBuf::from("/"))]);
    assert_eq!(ended.analytics, [Some("needs_action".to_string())]);
    let recorded = crate::mcp::listing_errors::snapshot()
        .into_iter()
        .filter(|e| e.listing_id == ended.listing.id())
        .count();
    assert_eq!(
        recorded, 1,
        "the read records the restricted root in the MCP ring itself"
    );
    assert!(ended.listing.is_cached(), "the empty listing still lands");
}
