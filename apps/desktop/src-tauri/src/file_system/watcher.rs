//! File system watcher with debouncing, incremental processing, and diff computation.
//!
//! Watches directories for changes and hands what it hears to the listing engine,
//! which patches the cache and publishes `directory-diff`s. Two processing paths:
//! incremental (stat each event's path, then `Listings::observe_children`) and
//! full re-read (`Listings::refresh`), the fallback for over 500 events, unknown
//! event kinds, or a watch root that changed identity.

use crate::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::entry::FileEntry;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, new_debouncer_opt,
    notify::{
        self, RecommendedWatcher, RecursiveMode,
        event::{EventKind, ModifyKind},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, SystemTime};
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::file_system::volume::backends::{get_single_entry, list_directory_core};
use cmdr_fs::firmlinks;
use cmdr_fs::volume::WatchCoverage;
use cmdr_listing::ListingId;
use cmdr_listing::host::{ListingObserver, OpenedListing};
use cmdr_listing::{Listings, RefreshOutcome};

/// Default debounce duration in milliseconds (used if not configured)
const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Configured debounce duration in milliseconds (set by frontend via update_debounce_ms)
static DEBOUNCE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(DEFAULT_DEBOUNCE_MS);

/// Updates the file watcher debounce duration.
/// This affects newly started watchers; existing watchers keep their original duration.
pub fn update_debounce_ms(ms: u64) {
    DEBOUNCE_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
    log::debug!("File watcher debounce updated to {} ms", ms);
}

/// Gets the current debounce duration in milliseconds.
fn get_debounce_ms() -> u64 {
    DEBOUNCE_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Global watcher manager
pub(crate) static WATCHER_MANAGER: LazyLock<RwLock<WatcherManager>> =
    LazyLock::new(|| RwLock::new(WatcherManager::new()));

/// `directory-deleted` event: the watched directory itself was deleted.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "directory-deleted")]
pub struct DirectoryDeletedEvent {
    pub listing_id: String,
    pub path: String,
}

/// State for a watched directory.
/// NOTE: No `entries` field - we use the unified the listing cache instead.
pub(crate) struct WatchedDirectory {
    #[allow(dead_code, reason = "Debouncer must be held to keep watching")]
    debouncer: Debouncer<RecommendedWatcher, NoCache>,
    /// What this particular watch observes, decided once when it was armed.
    ///
    /// Resolved here rather than in `Volume::listing_watch_coverage` because the
    /// answer needs a `statfs` on the path, and the oracle that asks for it runs
    /// inside sync recursive scan walkers, once per directory. Deciding at arm
    /// time costs one syscall per open listing, on a path whose `read_dir` has
    /// just succeeded, and leaves the read side a pure in-memory lookup.
    ///
    /// Private on purpose: `coverage_for_listings` is the only reader, so the
    /// backend asks a function rather than reaching into the manager's map.
    coverage: WatchCoverage,
}

/// Manages file watchers for directories
pub(crate) struct WatcherManager {
    pub(crate) watches: HashMap<String, WatchedDirectory>,
    pub(crate) app_handle: Option<AppHandle>,
}

impl WatcherManager {
    fn new() -> Self {
        Self {
            watches: HashMap::new(),
            app_handle: None,
        }
    }
}

/// Initialize the watcher manager with the app handle.
/// Must be called during app setup.
pub fn init_watcher_manager(app: AppHandle) {
    WATCHER_MANAGER.write_ignore_poison().app_handle = Some(app);
}

/// Whether the app handle is registered yet (set once during app setup).
///
/// A watcher whose only job is to refresh open frontend listings has nothing to
/// do before this is true: the diff emit is a no-op with no app handle. The
/// archive content watch uses this to skip starting an OS watch in headless /
/// pre-init contexts (unit tests), which also keeps the test suite from
/// oversubscribing FSEvents. Production sets the handle at startup, before any
/// browsing, so archive watches always start when a user opens a `.zip`.
pub fn app_handle_present() -> bool {
    WATCHER_MANAGER.read_ignore_poison().app_handle.is_some()
}

/// Start watching a directory for a given listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from list_directory_start
/// * `path` - The directory path to watch
///
/// Note: Initial entries are read from the listing cache when needed.
pub fn start_watching(listing_id: &str, path: &Path) -> Result<(), String> {
    log::debug!("start_watching: listing_id={}, path={}", listing_id, path.display());
    let listing_id_owned = listing_id.to_string();
    let listing_for_closure = listing_id_owned.clone();

    // Create the debouncer with a callback that handles changes
    let debounce_duration = Duration::from_millis(get_debounce_ms());
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        debounce_duration,
        None, // No tick rate limit
        move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    handle_directory_change_incremental(&listing_for_closure, events);
                }
                Err(_errors) => {
                    // Watcher errors often mean the watched directory was deleted.
                    // Try to re-read; if it fails with NotFound, we'll emit directory-deleted.
                    let lid = listing_for_closure.clone();
                    tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
                }
            }
        },
        // `NoCache`, not the platform default `RecommendedCache` (a `FileIdMap` on
        // macOS). The map exists to pair a rename's `From` with its `To` by file id, and
        // it pays for that by walking the watched directory and `stat`ing every entry at
        // arm time, then re-`stat`ing on every create, rename, and remove.
        //
        // We get nothing for it: `handle_directory_change_incremental` collects the
        // unique paths out of a batch and re-stats each one, so a rename classifies
        // identically whether it arrives as one paired event carrying both paths or as a
        // separate `From` and `To`. Root-rename detection is unaffected too, since
        // `watch_root_identity_changed` matches on `Modify(Name(_))`, which the debouncer
        // emits either way. Linux already runs this path with `NoCache`.
        NoCache,
        notify::Config::default(),
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Start watching the path (Debouncer implements Watcher trait)
    debouncer
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    // Store in manager (no entries - we use the listing cache)
    let coverage = coverage_for_watched_path(path);
    let mut manager = WATCHER_MANAGER.write().map_err(|_| "Failed to acquire watcher lock")?;

    manager
        .watches
        .insert(listing_id_owned, WatchedDirectory { debouncer, coverage });

    Ok(())
}

/// What an FSEvents watch on `path` can actually see.
///
/// A network mount answers [`WatchCoverage::ThisMachineOnly`]: FSEvents is a
/// local-VFS notifier, so it reports this machine's writes through the mount and
/// nothing another client does to the share. The pane still updates from the
/// user's own work, which is why the watch is worth arming at all; the oracle
/// just can't treat that cache as a substitute for reading the directory.
fn coverage_for_watched_path(path: &Path) -> WatchCoverage {
    if crate::file_system::index_provider::path_is_on_network_mount(path) {
        WatchCoverage::ThisMachineOnly
    } else {
        WatchCoverage::EveryWriter
    }
}

/// The coverage recorded for any live watch on `listing_ids`, best answer first.
///
/// Backs `LocalPosixVolume::listing_watch_coverage`, so it must stay a pure
/// in-memory read: it runs once per directory inside recursive scan walkers.
pub(crate) fn coverage_for_listings(listing_ids: &[String]) -> WatchCoverage {
    let manager = WATCHER_MANAGER.read_ignore_poison();
    let mut best = WatchCoverage::None;
    for id in listing_ids {
        match manager.watches.get(id.as_str()).map(|w| w.coverage) {
            // Nothing beats full coverage, so stop at the first one.
            Some(WatchCoverage::EveryWriter) => return WatchCoverage::EveryWriter,
            Some(WatchCoverage::ThisMachineOnly) => best = WatchCoverage::ThisMachineOnly,
            Some(WatchCoverage::None) | None => {}
        }
    }
    best
}

/// Arms the listing watcher without making the caller wait for it.
///
/// ❌ Don't call [`start_watching`] from the listing pipeline instead. Arming is slow
/// and, worse, slow by an amount that has nothing to do with the directory being
/// listed: it waits on an `FSEventStreamStart` handshake with `fseventsd`, on a
/// CFRunLoop thread bootstrap, and on [`stop_watching`] finishing the PREVIOUS
/// listing's teardown. The pipeline arms the watch before it emits `listing-complete`
/// and the pane renders nothing until that event, so every one of those waits used to
/// be dead time the user saw as a stalled "Sorting your files, preparing view…"
/// (measured p50 88 ms, p90 653 ms, max 1.5 s while navigating a warm `~/Downloads`,
/// macOS 26.5.2, 2026-08-11; a 3-entry folder hit 723 ms and a 265-entry folder 57 ms,
/// which is what "not about the directory" looks like in the data).
///
/// The listing doesn't depend on the watch: entries are read, sorted, and cached
/// before this is called. The watch only has to be in place before the user notices a
/// change on disk. A change that lands between the read and the arm is caught by
/// [`arm_and_reconcile`]'s mtime check, since the new stream only reports what happens
/// after it starts.
pub fn start_watching_detached(listing_id: &str, path: &Path) {
    let listing_id = listing_id.to_string();
    let path = path.to_path_buf();
    // Taken here, on the open path, not inside the task: the blocking pool can queue the
    // task under load, and a change in that wait must still count as "after the open".
    let opened_at = SystemTime::now();
    // `spawn_blocking`, not `spawn`: arming is a chain of blocking syscalls, so it must
    // not sit on a runtime worker. `tauri::async_runtime` rather than `tokio` because
    // the sync `list_directory_start` path calls this from the IPC handler thread,
    // which has no Tokio runtime context.
    tauri::async_runtime::spawn_blocking(move || arm_and_reconcile(&listing_id, &path, opened_at, arm_listing_watch));
}

/// [`start_watching`], after the latch an E2E spec may hold
/// (`test_mode::wait_for_listing_arm_release`, a no-op outside E2E mode). The latch sits
/// inside the arm, after `arm_and_reconcile`'s first stat, which is exactly where a slow
/// fseventsd handshake puts the gap in production.
fn arm_listing_watch(listing_id: &str, path: &Path) -> Result<(), String> {
    crate::test_mode::wait_for_listing_arm_release();
    start_watching(listing_id, path)
}

/// The coarsest directory mtime a volume Cmdr watches can keep: FAT and some SMB
/// servers store 2 s ticks. APFS stores nanoseconds.
const MTIME_GRANULARITY: Duration = Duration::from_secs(2);

/// Arms the watch, hands it back if the listing ended while we were arming, and
/// re-reads the listing if the directory changed before the stream started.
///
/// **Liveness.** `list_directory_end` removes the listing from the listing cache and
/// then removes a watch that a detached arm may not have inserted yet, so the arm can
/// land on a listing nobody will ever close again. Left alone that strands an FSEvents
/// stream, its CFRunLoop thread, and a manager entry for the life of the process, each
/// still costing `fseventsd` fan-out. The listing cache membership is the signal because
/// `list_directory_end` is what clears it, and reading it is a plain lookup that doesn't
/// bump `last_accessed_ms` (this is background work, not user activity — see
/// `crates/cmdr-listing/CLAUDE.md`).
///
/// **The change the stream can't see.** The stream starts "since now", and under
/// fseventsd load the arm lands seconds after the read, so a create in that window was
/// never shown until the user navigated away and back. One `stat` before and after the
/// arm decides it ([`arm_missed_a_change`]); the full re-read runs only when the mtime
/// says it must, so a huge folder doesn't pay a second readdir per navigation.
///
/// `arm` is [`start_watching`] in production; a test passes one that makes a change land
/// while the arm is in flight.
pub(super) fn arm_and_reconcile(
    listing_id: &str,
    path: &Path,
    opened_at: SystemTime,
    arm: impl FnOnce(&str, &Path) -> Result<(), String>,
) {
    let mtime_before = dir_mtime(path);
    if let Err(e) = arm(listing_id, path) {
        log::warn!("Failed to start watcher: {}", e);
        return;
    }

    if !crate::listing_host::listings().contains(&ListingId::new(listing_id)) {
        log::debug!(
            "start_watching_detached: listing {} ended while arming, dropping the watch",
            listing_id
        );
        stop_watching(listing_id);
        return;
    }

    if arm_missed_a_change(mtime_before, dir_mtime(path), opened_at) {
        log::debug!(
            "start_watching_detached: {} may have changed before its watch started, re-reading",
            path.display()
        );
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn`: this runs on the blocking pool, outside any
        // Tokio context (see `handle_directory_change_incremental`).
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
    }
}

fn dir_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Whether the directory may have changed between the listing's read and its watch
/// starting, so the listing must be re-read.
///
/// - The mtime moved across the arm: a child was created, removed, or renamed.
/// - The mtime before the arm is within [`MTIME_GRANULARITY`] of the open, or later. A
///   change in the same coarse tick as the last one leaves the mtime unchanged, and so
///   does one between the read and the first stat, so a recently touched directory
///   can't be trusted to have stood still. Real navigation rarely lands here.
/// - Either `stat` failed: the re-read sorts out a vanished or unreadable directory.
///
/// Edits inside an existing child don't touch the directory's mtime, so a size change in
/// the window is still missed; the next event on that file corrects it. Assumes the
/// volume's clock agrees with ours within a tick; a server clock running ahead only
/// costs extra re-reads.
pub(super) fn arm_missed_a_change(
    mtime_before: Option<SystemTime>,
    mtime_after: Option<SystemTime>,
    opened_at: SystemTime,
) -> bool {
    let (Some(before), Some(after)) = (mtime_before, mtime_after) else {
        return true;
    };
    if before != after {
        return true;
    }
    match opened_at.checked_sub(MTIME_GRANULARITY) {
        Some(trust_from) => before >= trust_from,
        None => true,
    }
}

/// The change watch, as the listing engine's first observer: a listing on a
/// volume that can carry a watch arms one on open (detached), and every close
/// disarms, after the cache removal like every observer.
struct WatchedListings;

/// The watch observer, for the listing host to install ahead of every other.
pub(crate) fn observer() -> Arc<dyn ListingObserver> {
    Arc::new(WatchedListings)
}

impl ListingObserver for WatchedListings {
    fn id(&self) -> &'static str {
        "fs-watcher"
    }

    /// A routed volume answers `false` to `can_watch_listings` (a git
    /// snapshot's and an archive's paths aren't on disk, so `notify` has
    /// nothing to arm on), which keeps every virtual path out by type.
    fn listing_opened(&self, opened: &OpenedListing<'_>) {
        if opened.volume.can_watch_listings() {
            start_watching_detached(opened.id.as_str(), opened.path);
        }
    }

    fn listing_closed(&self, id: &ListingId) {
        stop_watching(id.as_str());
    }
}

/// Stop watching a directory for a given listing.
pub fn stop_watching(listing_id: &str) {
    #[cfg(test)]
    crate::listing_host::tests::support::note_disarm(listing_id);
    let removed = WATCHER_MANAGER.write_ignore_poison().watches.remove(listing_id);

    // Drop OUTSIDE the lock. Dropping a `WatchedDirectory` tears an FSEvents run loop
    // down, and notify's teardown busy-spins on `CFRunLoopIsWaiting` before it joins the
    // stream's thread. Holding the manager's write lock across that made every
    // navigation's arm queue behind the previous listing's teardown, since the frontend
    // fires `listDirectoryEnd(old)` immediately before loading the new directory.
    drop(removed);
}

/// Maps an FSEvents/inotify path to the watched listing's path space, returning the
/// rebased path when the event is for a direct child of the watched directory.
///
/// Two path-form mismatches make a raw `parent == dir_path` comparison silently drop
/// events, leaving the pane stale until the user re-navigates:
///
/// 1. **Firmlinks / well-known `/private` symlinks.** FSEvents reports canonical paths
///    (`/private/tmp/…`) while the listing cache holds the user-navigated form
///    (`/tmp/…`). `firmlinks::normalize_path` (the same canonicalization the index
///    uses) aligns both.
/// 2. **A symlinked watch root.** Google Drive exposes "My Drive" as a symlink
///    (`…/CloudStorage/GoogleDrive-…/My Drive` → `~/My Drive`), and FSEvents resolves
///    the watched symlink and reports events under the real target. `canonical_dir` is
///    `dir_path` with its symlinks resolved (see the caller); matching against it
///    catches this case. iCloud and Dropbox mount real directories, so they hit the
///    firmlink path above; only Google Drive needs this branch.
///
/// The returned path is always rebased onto `dir_path` so cache lookups (`has_entry`)
/// and diff entries stay consistent with the listing's own path space.
pub(super) fn rebase_event_path(event_path: &Path, dir_path: &Path, canonical_dir: &Path) -> Option<PathBuf> {
    let parent = event_path.parent()?;
    if parent == dir_path {
        return Some(event_path.to_path_buf());
    }
    let parent_normalized = firmlinks::normalize_path(&parent.to_string_lossy());
    let dir_normalized = firmlinks::normalize_path(&dir_path.to_string_lossy());
    let canonical_normalized = firmlinks::normalize_path(&canonical_dir.to_string_lossy());
    if parent_normalized == dir_normalized || parent_normalized == canonical_normalized {
        event_path.file_name().map(|name| dir_path.join(name))
    } else {
        None
    }
}

/// Whether an FSEvents/inotify path IS the watched directory itself rather than
/// something inside it, in either path form (see `rebase_event_path` for why a
/// listing's path and the OS's path can differ).
pub(super) fn event_targets_watch_root(event_path: &Path, dir_path: &Path, canonical_dir: &Path) -> bool {
    if event_path == dir_path || event_path == canonical_dir {
        return true;
    }
    let event_normalized = firmlinks::normalize_path(&event_path.to_string_lossy());
    event_normalized == firmlinks::normalize_path(&dir_path.to_string_lossy())
        || event_normalized == firmlinks::normalize_path(&canonical_dir.to_string_lossy())
}

/// Whether this batch says the watched directory itself was replaced, removed, or
/// renamed, so the pane's entries can no longer be trusted.
///
/// `Modify(Metadata(_))` on the root is deliberately NOT a trigger: every ordinary
/// child create or remove bumps the directory's own mtime, so counting it would send
/// every change down the full re-read path and cost the incremental path its point.
fn watch_root_identity_changed(events: &[DebouncedEvent], dir_path: &Path, canonical_dir: &Path) -> bool {
    events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        ) && event
            .paths
            .iter()
            .any(|path| event_targets_watch_root(path, dir_path, canonical_dir))
    })
}

/// Processes individual file-system events incrementally instead of re-reading the whole directory.
///
/// Falls back to `handle_directory_change` when events are too numerous or ambiguous.
pub(super) fn handle_directory_change_incremental(listing_id: &str, events: Vec<DebouncedEvent>) {
    // Fallback: too many events or ambiguous event kinds
    if events.len() > 500
        || events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Any | EventKind::Other))
    {
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn` instead of `tokio::spawn` because this
        // closure runs on the notify-rs debouncer thread, which has no Tokio
        // runtime context. Tauri's async runtime works from any thread.
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
        return;
    }

    // The watched directory's path, from the cache (without cloning its entries).
    let Some((_, dir_path)) = crate::listing_host::listings().location(&ListingId::new(listing_id)) else {
        return;
    };

    // Resolve the watched dir's symlinks once per batch, so events FSEvents reports
    // under a symlinked root's real target (Google Drive's "My Drive" → `~/My Drive`)
    // still match. This is a `realpath` syscall like the per-event `get_single_entry`
    // stats below, so it adds no new blocking class here; falls back to `dir_path` if
    // the dir vanished mid-batch (the re-read path handles a deleted watch root).
    let canonical_dir = std::fs::canonicalize(&dir_path).unwrap_or_else(|_| dir_path.clone());

    // A watch root that was itself removed, created, or renamed is a DIFFERENT
    // directory now, so classifying this batch's child events against the old entries
    // would keep everything the replacement took away. macOS reports a wholesale
    // replacement (a `git checkout` across branches, `rsync --delete`, unzipping over a
    // folder, a build regenerating its output dir) as Remove(Folder) + Create(Folder)
    // on the ROOT plus one Create per NEW child, and never a remove for the old ones.
    // Re-read instead: that diffs against disk, replaces the listing, and emits
    // `directory-deleted` plus stops the watch when the directory is really gone.
    if watch_root_identity_changed(&events, &dir_path, &canonical_dir) {
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn`, not `tokio::spawn`: see the fallback above.
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
        return;
    }

    // Collect unique direct-child paths, skipping access events. Event paths are
    // rebased into the listing's path space (see `rebase_event_path`).
    let mut unique_paths: HashSet<PathBuf> = HashSet::new();
    for event in &events {
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        for path in &event.paths {
            if let Some(rebased) = rebase_event_path(path, &dir_path, &canonical_dir) {
                unique_paths.insert(rebased);
            }
        }
    }

    if unique_paths.is_empty() {
        return;
    }

    // Stat every path BEFORE handing the batch over: the engine takes its locks
    // after, and a stat on a slow disk must not sit inside one.
    let children: Vec<(PathBuf, Option<FileEntry>)> = unique_paths
        .into_iter()
        .map(|path| {
            let entry = get_single_entry(&path).ok();
            (path, entry)
        })
        .collect();

    crate::listing_host::listings().observe_children(&ListingId::new(listing_id), children);
}

/// Force a re-read of a directory listing, computing and emitting any diff.
/// Called by the file watcher on change events, and also available as a Tauri
/// command for cases where the watcher doesn't fire (e.g. rename-move on Linux).
///
/// The engine re-reads through the listing's volume (`Listings::refresh`); this
/// wrapper owns what the app does with the outcome: `directory-deleted` plus
/// `stop_watching` when the directory is gone, and the `std::fs` re-read when
/// nothing serves the volume any more. Returns once the re-read landed.
pub async fn handle_directory_change(listing_id: &str) {
    log::debug!("handle_directory_change: listing_id={}", listing_id);

    let listings = crate::listing_host::listings();
    let id = ListingId::new(listing_id);
    match listings.refresh(&id).await {
        RefreshOutcome::DirectoryGone(path) => directory_deleted(listing_id, &path),
        // Volume unregistered: fall back to std::fs for local paths.
        RefreshOutcome::NoVolume => reread_with_std_fs(listings, &id).await,
        RefreshOutcome::Landed | RefreshOutcome::Superseded | RefreshOutcome::ListingGone | RefreshOutcome::Skipped => {
        }
    }
}

/// The re-read for a listing whose volume nothing serves any more: `std::fs`,
/// handed to the engine. The ticket is taken BEFORE the read, so it dates the read.
async fn reread_with_std_fs(listings: &Listings, id: &ListingId) {
    let Some(ticket) = listings.begin_read(id) else {
        return;
    };
    match list_directory_core(ticket.path()) {
        Ok(entries) => {
            listings.refresh_from_entries(ticket, entries).await;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let path = ticket.path().to_path_buf();
            directory_deleted(id.as_str(), &path);
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::PermissionDenied {
                log::warn!("Watcher: Failed to re-read directory: {}", e);
            }
        }
    }
}

/// The watched directory itself is gone: tell the pane, and stop watching it.
fn directory_deleted(listing_id: &str, path: &Path) {
    log::info!("Watcher: Directory deleted, notifying frontend: {}", path.display());
    let app_handle = { WATCHER_MANAGER.read_ignore_poison().app_handle.clone() };
    if let Some(app) = &app_handle {
        let event = DirectoryDeletedEvent {
            listing_id: listing_id.to_string(),
            path: path.to_string_lossy().to_string(),
        };
        if let Err(emit_err) = event.emit(app) {
            log::warn!("Watcher: Failed to emit directory-deleted event: {}", emit_err);
        }
    }
    stop_watching(listing_id);
}

/// Flushes pending watcher events by re-reading every active watch.
///
/// `notify-debouncer-full` doesn't expose a synchronous flush, and the
/// debouncer's window (plus FSEvents coalescing on macOS) adds 1–10 s of
/// latency per FS mutation under E2E. This helper sidesteps the debouncer:
/// it grabs every active listing_id, then `handle_directory_change` re-reads
/// each one via the Volume trait, computes the diff, updates the listing cache,
/// and emits a `directory-diff` event.
///
/// Feature-gated to `playwright-e2e` so production builds can't accidentally
/// bypass the debouncer (which exists to prevent thrash on bursts of events;
/// tests don't need that: they need determinism).
#[cfg(feature = "playwright-e2e")]
pub async fn flush_all_watchers() {
    let listing_ids: Vec<String> = WATCHER_MANAGER.read_ignore_poison().watches.keys().cloned().collect();
    log::debug!("flush_all_watchers: flushing {} watches", listing_ids.len());
    for id in listing_ids {
        handle_directory_change(&id).await;
    }
    // handle_directory_change now enqueues into the coalescer; flush so the
    // emit happens before this returns (E2E callers expect synchronous flush).
    crate::listing_host::listings().flush_pending_diffs();
}
