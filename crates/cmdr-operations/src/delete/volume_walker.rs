//! The volume-aware delete (MTP, SMB, a `LocalPosixVolume` folder, anything
//! behind a `Volume`): an oracle-aware recursive listing, then `Volume::delete`
//! per file or `delete_files` per batch. The local-FS twin is `walker.rs`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::event_sinks::OperationEventSink;
use super::super::scan_preview::kind_in_parent_listing;
use super::super::state::WriteOperationState;
use super::super::transfer::volume_outage::StartedOn;
use super::super::types::{
    CancelRollback, DryRunResult, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};
use super::outage::{EntryStop, Listed, delete_entry, read_through_outages};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{EntryKind, Volume};

/// Entry collected during the volume scan phase. `progress_bytes` is the
/// progress contribution: equals the size for the first occurrence of an inode
/// in the scan, `0` for hardlink dupes. Mirrors `FileInfo::progress_bytes` on
/// the local-FS walker. Volume backends without inode info (MTP, SMB) populate
/// this as the size for every entry — they never produce hardlinks, so no
/// dedup happens and the count is correct. `listed` is what a delete re-run
/// after an outage must still find at `path` (`outage.rs`).
struct VolumeDeleteEntry {
    path: PathBuf,
    progress_bytes: u64,
    is_dir: bool,
    listed: Listed,
}

/// Tracks the running tally across the whole recursive scan so the per-entry
/// `list_directory` callback (which can fire while a single dir is still
/// streaming entries from a slow MTP USB roundtrip) reads a coherent total.
struct VolumeScanTracker {
    /// Last emit timestamp for the global throttle (shared across recursion levels).
    last_emit: Mutex<Instant>,
    progress_interval: Duration,
    /// Files committed to `entries` so far. The per-entry callback adds the
    /// in-flight `loaded_count` on top to form the displayed tally.
    files_so_far: std::sync::atomic::AtomicUsize,
    dirs_so_far: std::sync::atomic::AtomicUsize,
    bytes_so_far: std::sync::atomic::AtomicU64,
}

impl VolumeScanTracker {
    fn new(progress_interval: Duration) -> Self {
        Self {
            last_emit: Mutex::new(Instant::now()),
            progress_interval,
            files_so_far: std::sync::atomic::AtomicUsize::new(0),
            dirs_so_far: std::sync::atomic::AtomicUsize::new(0),
            bytes_so_far: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Returns true if enough time has passed since the last emit AND atomically
    /// resets the timer. Single-call gate so callers don't accidentally double-emit.
    fn should_emit(&self) -> bool {
        // A throttle timestamp: a poisoned one is still a timestamp.
        let mut last = self.last_emit.lock_ignore_poison();
        if last.elapsed() < self.progress_interval {
            return false;
        }
        *last = Instant::now();
        true
    }
}

/// Recursively enumerates a directory tree via `volume.list_directory()`, collecting
/// files and directories. Directories are appended after their children, so the
/// resulting list is already in deepest-first order for safe deletion.
///
/// **A symlink is a leaf: the link goes, ❌ never its target's contents.** Only
/// an [`EntryKind::Directory`] is listed and descended. A listing sets
/// `is_directory` on a link to a folder (panes want it to navigate like one), and
/// some backends' `is_directory` follows the link, so neither may decide a
/// descent: `kind_hint` is an [`EntryKind`], read off an entry with
/// [`EntryKind::of`] or asked of [`Volume::entry_kind`].
///
/// Emits scan progress two ways:
/// 1. **Per-entry** via `list_directory`'s `on_progress` callback, so the FE sees the tally climb
///    mid-listing — important for MTP, where one `list_directory` call on `/DCIM/Camera` with 1k+
///    photos can take ~17 s of USB roundtrips.
/// 2. **After each subtree** (this function's tail), as a final snapshot for that branch. Throttled
///    by the shared `VolumeScanTracker` so it doesn't race with the per-entry callback.
///
/// **Cancel contract**: this function does NOT emit `write-cancelled` before returning
/// `Err(Cancelled)`. The cancel check fires at every recursion level, so emitting here would
/// multi-fire. Top-level callers must call `emit_cancelled_if_aborted` (or otherwise emit
/// `write-cancelled`) on the returned result before propagating it, so the FE sees the proper
/// terminal event for the cancel flow.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
async fn scan_volume_recursive(
    volume: &Arc<dyn Volume>,
    volume_id: &str,
    path: &Path,
    kind_hint: Option<EntryKind>,
    entries: &mut Vec<VolumeDeleteEntry>,
    total_bytes: &mut u64,
    seen_inodes: &mut std::collections::HashSet<u64>,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &Arc<VolumeScanTracker>,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    if state.intent().is_cancelled() {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Resolve what `path` is. Prefer the caller's hint (a pane's listing of the
    // parent at the top level, the parent's `FileEntry` in recursion). Without
    // one, ask the volume's link-aware `entry_kind` (a read, so safe to re-run
    // after an outage), which on MTP lists the parent dir, so we avoid it
    // whenever the oracle can answer.
    let kind = match kind_hint {
        Some(kind) => kind,
        None => read_through_outages(volume, state, || volume.entry_kind(path))
            .await
            .map_err(|stop| stop.into_error(path))?,
    };

    if kind == EntryKind::Directory {
        super::super::mount_boundary::refuse_a_volume_mount_point(&**volume, path, state).await?;

        // Snapshot the cumulative tallies BEFORE the (potentially slow) listing
        // call. The per-entry callback shows `files_before + loaded_count` so the
        // FE sees a climbing number even when one `list_directory` takes seconds.
        // We don't know mid-stream whether each entry is a file or dir, so
        // optimistically attribute them to `files`; the post-listing loop below
        // corrects the split via the `dirs_so_far` counter.
        let files_before = tracker.files_so_far.load(Ordering::Relaxed);
        let dirs_before = tracker.dirs_so_far.load(Ordering::Relaxed);
        let bytes_before = tracker.bytes_so_far.load(Ordering::Relaxed);
        let current_dir_str = path.display().to_string();
        let op_id_for_cb = operation_id.to_string();
        let tracker_for_cb = Arc::clone(tracker);
        let state_for_cb = Arc::clone(state);

        let on_progress = move |p: cmdr_fs::volume::ListingProgress| {
            if !tracker_for_cb.should_emit() {
                return;
            }
            // The delete scan tally tracks files + bytes from already-walked
            // subdirs; the volume's per-listing callback adds the local
            // (files, bytes) it's enumerating right now.
            let files_now = files_before + p.files;
            let bytes_now = bytes_before + p.bytes;
            state_for_cb.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    op_id_for_cb.clone(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Scanning,
                    None,
                    files_now,
                    0,
                    bytes_now,
                    0,
                )
                .with_scan_meta(Some(current_dir_str.clone()), dirs_before + p.dirs, None),
            );
            state.scope().status().update(
                &op_id_for_cb,
                WriteOperationPhase::Scanning,
                None,
                files_now,
                0,
                bytes_now,
                0,
            );
        };

        // Oracle-first: if this directory is watcher-backed in the listing cache,
        // reuse the cached entries and skip the volume round-trip entirely.
        // Falls through to `volume.list_directory` on miss, preserving the
        // per-entry progress callback for slow MTP listings.
        let children = match state.scope().host().listings().authoritative_listing(volume_id, path) {
            Some(cached) => {
                // The cached listing is already complete, so synthesize a
                // single end-of-listing progress tick (with the tally we'd
                // have built incrementally) to keep the FE counter climbing
                // during the cache-fed pass too.
                let mut tally = cmdr_fs::volume::ListingProgress::default();
                for e in &cached {
                    if e.is_directory {
                        tally.dirs += 1;
                    } else {
                        tally.files += 1;
                        tally.bytes += e.size.unwrap_or(0);
                    }
                }
                on_progress(tally);
                cached
            }
            None => read_through_outages(volume, state, || {
                volume.list_directory_with_cancel(path, Some(&on_progress), Some(&state.backend_cancel))
            })
            .await
            .map_err(|stop| stop.into_error(path))?,
        };

        // Recurse into children first. list_directory returns FileEntry with size,
        // so we use child.size directly instead of calling get_metadata (which
        // returns NotSupported on MTP). Pass the child's own kind as the hint so
        // the recursive call doesn't re-probe (avoids another parent listing on
        // MTP). `EntryKind::of`, ❌ never `child.is_directory`: that's true for a
        // link to a folder, and descending it deletes the folder it points at.
        for child in &children {
            let child_path = PathBuf::from(&child.path);
            let child_kind = EntryKind::of(child);
            if child_kind == EntryKind::Directory {
                Box::pin(scan_volume_recursive(
                    volume,
                    volume_id,
                    &child_path,
                    Some(EntryKind::Directory),
                    entries,
                    total_bytes,
                    seen_inodes,
                    state,
                    events,
                    operation_id,
                    tracker,
                ))
                .await?;
            } else {
                // A link frees only itself, whatever size the listing reports
                // for what it points at.
                let size = if child_kind == EntryKind::Symlink {
                    0
                } else {
                    child.size.unwrap_or(0)
                };
                // Hardlink dedup: when the backend reports an inode, only
                // count the first occurrence toward `total_bytes` and the
                // tracker's running byte total. Subsequent hardlinks
                // contribute `progress_bytes = 0` so the delete loop's
                // numerator stays aligned with the denominator. Backends
                // without inode info (MTP, SMB) leave `inode = None` and
                // every entry is treated as unique — they never produce
                // hardlinks anyway. Mirrors `FileInfo::progress_bytes` on
                // the local-FS walker.
                let counts = match child.inode {
                    Some(ino) => seen_inodes.insert(ino),
                    None => true,
                };
                let progress_bytes = if counts { size } else { 0 };
                if counts {
                    *total_bytes += size;
                    tracker.bytes_so_far.fetch_add(size, Ordering::Relaxed);
                }
                tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                entries.push(VolumeDeleteEntry {
                    path: child_path,
                    progress_bytes,
                    is_dir: false,
                    listed: Listed {
                        kind: EntryKind::of(child),
                        size: child.size,
                    },
                });
            }
        }

        // Add directory after its children (already deepest-first order)
        tracker.dirs_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: true,
            listed: Listed {
                kind: EntryKind::Directory,
                size: None,
            },
        });
    } else {
        // Top-level file or link without listing context: size unknown, use 0.
        // Progress still tracks file count accurately.
        tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
        entries.push(VolumeDeleteEntry {
            path: path.to_path_buf(),
            progress_bytes: 0,
            is_dir: false,
            listed: listed_top_level_file(volume.as_ref(), path).await,
        });
    }

    // Final snapshot for this subtree (throttled so it doesn't race the per-entry callback).
    if tracker.should_emit() {
        let files_done = tracker.files_so_far.load(Ordering::Relaxed);
        let dirs_done = tracker.dirs_so_far.load(Ordering::Relaxed);
        let bytes_done = tracker.bytes_so_far.load(Ordering::Relaxed);
        let current_dir_str = path.display().to_string();
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Delete,
                WriteOperationPhase::Scanning,
                None,
                files_done,
                0,
                bytes_done,
                0,
            )
            .with_scan_meta(Some(current_dir_str), dirs_done, None),
        );
        state.scope().status().update(
            operation_id,
            WriteOperationPhase::Scanning,
            None,
            files_done,
            0,
            bytes_done,
            0,
        );
    }

    Ok(())
}

/// What a top-level file the walk only probed for its type is, for a delete
/// re-run after an outage to check against. One `get_metadata`, and only on a
/// volume with a session: no other volume is ever waited for, so nothing would
/// read it. Best effort: an unanswered stat leaves the size unknown, and a
/// re-run then leaves the file rather than delete it unproven.
async fn listed_top_level_file(volume: &dyn Volume, path: &Path) -> Listed {
    let unknown = Listed {
        kind: EntryKind::File,
        size: None,
    };
    if volume.connection_state().is_none() {
        return unknown;
    }
    match volume.get_metadata(path).await {
        Ok(entry) => Listed {
            kind: EntryKind::of(&entry),
            size: entry.size,
        },
        Err(_) => unknown,
    }
}

/// Deletes `batch` in one `delete_files` call, then takes each file the call
/// couldn't delete into its own outage loop (`outage::delete_entry`), starting
/// at the wait: a file the drop cut off is re-checked and deleted once the
/// share is back. After the first failure for good, the rest keep the batch's
/// own answer: the delete ends there anyway.
async fn delete_batch(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    batch: &[&VolumeDeleteEntry],
) -> Vec<Result<(), EntryStop>> {
    let paths: Vec<PathBuf> = batch.iter().map(|entry| entry.path.clone()).collect();
    let started_on = StartedOn::now(&[volume]);
    let answers = volume.delete_files(&paths).await;
    let mut results = Vec::with_capacity(batch.len());
    let mut stopped = false;
    for (entry, answer) in batch.iter().zip(answers) {
        let result = match answer {
            Ok(()) => Ok(()),
            Err(error) if stopped => Err(EntryStop::Failed { error, lost: None }),
            Err(error) => {
                delete_entry(
                    volume,
                    state,
                    &entry.path,
                    entry.listed,
                    Some((error, started_on.clone())),
                )
                .await
            }
        };
        stopped |= result.is_err();
        results.push(result);
    }
    results
}

/// Emits `write-cancelled` when `result` is `Err(Cancelled)`. Use this after
/// every top-level `scan_volume_recursive` call so the FE sees the proper
/// terminal event before the error propagates up. See `scan_volume_recursive`'s
/// "Cancel contract" doc for why this is the caller's responsibility.
fn emit_cancelled_if_aborted(
    result: &Result<(), WriteOperationError>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &VolumeScanTracker,
) {
    if matches!(result, Err(WriteOperationError::Cancelled { .. })) {
        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_processed: tracker.files_so_far.load(std::sync::atomic::Ordering::Relaxed),
            rollback: CancelRollback::none(),
        });
    }
}

/// Deletes files on a non-local volume (like MTP) with progress reporting.
///
/// Uses `volume.list_directory()` for scanning, and `volume.delete()` per file or
/// `volume.delete_files()` per batch where the backend batches
/// (`Volume::delete_batch_size`), then `volume.delete_directory()` per folder.
/// Emits the same events as `delete_files_with_progress` so the frontend progress
/// dialog works unchanged.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(crate) async fn delete_volume_files_with_progress_inner(
    volume: Arc<dyn Volume>,
    volume_id: &str,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    // Phase 1: Scan. Recursively enumerate the tree via volume.list_directory().
    // Fast path: if the scan preview already enumerated the top-level shape (file
    // vs dir + size per source), reuse it. The per-source `top_level_is_directory`
    // and `total_bytes` flags come from `CachedScanResult::per_path` populated by
    // `run_volume_scan_preview`. We still need to recurse into top-level dirs to
    // get the per-file paths delete needs — but the recursion's walker is now
    // oracle-aware, so a watched subtree skips the `list_directory` round-trip.
    let previews = state.scope().stores().previews();
    let cached_scan = config
        .preview_id
        .as_deref()
        .and_then(|preview_id| previews.take_cached_scan_result(preview_id, sources));

    let mut entries: Vec<VolumeDeleteEntry> = Vec::new();
    let mut total_bytes = 0u64;
    // Operation-scoped hardlink dedup. Shared across all top-level sources
    // so a hardlink that spans two different sources still counts once
    // (matches the local-FS walker's contract — `seen_inodes` in
    // `scan.rs::walk_dir_recursive`). Empty for backends without inode info.
    let mut seen_inodes: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let tracker = Arc::new(VolumeScanTracker::new(state.progress_interval));
    let mut last_scan_emit = Instant::now();

    if let Some(scan) = cached_scan.as_ref() {
        log::debug!(
            "delete_volume_files_with_progress: reused cached scan for operation_id={}, files={}, bytes={}, per_path={}",
            operation_id,
            scan.file_count,
            scan.total_bytes,
            scan.per_path.len()
        );

        // Index cached per-path results by source path so we can look up
        // `top_level_is_directory` and `total_bytes` for each input source.
        let by_path: std::collections::HashMap<PathBuf, &cmdr_fs::volume::CopyScanResult> =
            scan.per_path.iter().map(|(p, r)| (p.clone(), r)).collect();

        for source in sources {
            if state.intent().is_cancelled() {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: 0,
                    rollback: CancelRollback::none(),
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            match by_path.get(source) {
                Some(per_path_scan) if !per_path_scan.top_level_is_directory => {
                    // Top-level file: size comes straight from the cache. No
                    // `is_directory` probe and no `list_directory` round-trip.
                    // No inode info on the cache-hit path (`CopyScanResult`
                    // doesn't carry it), so each top-level cached file is
                    // treated as a unique inode. Cross-source hardlinks at
                    // the top level aren't dedup'd; within-source recursion
                    // still dedupes via the walker's `seen_inodes`.
                    let size = per_path_scan.total_bytes;
                    tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                    tracker.bytes_so_far.fetch_add(size, Ordering::Relaxed);
                    total_bytes += size;
                    entries.push(VolumeDeleteEntry {
                        path: source.to_path_buf(),
                        progress_bytes: size,
                        is_dir: false,
                        listed: Listed {
                            kind: EntryKind::File,
                            size: Some(size),
                        },
                    });
                }
                Some(_) => {
                    // Top-level dir per the preview: recurse via the oracle-aware
                    // walker. Any subtree that's open in another pane and
                    // watcher-fresh gets cache-fed inside the walker.
                    //
                    // ❌ Don't hand it `Some(Directory)` on the preview's word: a
                    // preview's top-level stat may follow a link (a copy's does,
                    // by design), so "directory" can be a link to one. The parent
                    // listing answers link-aware with no I/O; failing that, the
                    // walker asks `entry_kind`.
                    let result = scan_volume_recursive(
                        &volume,
                        volume_id,
                        source,
                        kind_in_parent_listing(state.scope().host().listings(), volume_id, source),
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
                None => {
                    // Scan preview was non-volume (local-FS) or didn't include
                    // this source. Fall back to the no-preview shape for this
                    // path: the oracle-aware walker resolves the type.
                    //
                    // ❌ Don't "fix" the `None` here into a default. It looks
                    // like the bug this module's guardrails are about and is the
                    // opposite: forwarding `kind_hint: None` is what makes
                    // `scan_volume_recursive` PROPAGATE a failed probe
                    // (`map_err(...)?`) instead of guessing. A `Some(File)`
                    // here would be the guess.
                    let result = scan_volume_recursive(
                        &volume,
                        volume_id,
                        source,
                        None,
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
            }

            // Even on the all-files fast path, throttle a single scan-progress
            // emit per interval so the FE dialog shows movement during the
            // entry-list build. Without this the user sees "Scanning…" frozen
            // until the actual delete phase starts.
            if last_scan_emit.elapsed() >= state.progress_interval {
                let files_done = tracker.files_so_far.load(Ordering::Relaxed);
                let dirs_done = tracker.dirs_so_far.load(Ordering::Relaxed);
                let bytes_done = tracker.bytes_so_far.load(Ordering::Relaxed);
                state.emit_progress_via_sink(
                    events,
                    WriteProgressEvent::new(
                        operation_id.to_string(),
                        WriteOperationType::Delete,
                        WriteOperationPhase::Scanning,
                        None,
                        files_done,
                        0,
                        bytes_done,
                        0,
                    )
                    .with_scan_meta(Some(source.display().to_string()), dirs_done, None),
                );
                state.scope().status().update(
                    operation_id,
                    WriteOperationPhase::Scanning,
                    None,
                    files_done,
                    0,
                    bytes_done,
                    0,
                );
                last_scan_emit = Instant::now();
            }
        }
    } else {
        for source in sources {
            // Oracle-first parent lookup: if the parent's listing is watched
            // (open in some pane), take the entry's kind from there instead of
            // calling `volume.entry_kind`, which on MTP lists the parent. Falls
            // through to the trait probe when the oracle can't answer.
            let parent_hint = kind_in_parent_listing(state.scope().host().listings(), volume_id, source);

            // Hand the hint (or the lack of one) straight to the walker: it
            // resolves an absent hint with a PROPAGATING probe and handles both
            // the directory and the top-level-file case. Resolving it here
            // instead would mean owning a second answer to the same question,
            // and the obvious way to write that second answer is
            // `.unwrap_or(false)`, which turns an unanswerable stat into a
            // confident "file". Then the entry goes in as `is_dir: false` with
            // zero bytes, so the dialog describes one file and no bytes for
            // what may be a whole tree, and the `delete` that follows acts on a
            // fact nobody established. A source that can't be stat'd fails that
            // item instead, which is the honest outcome and cheap to retry.
            let result = scan_volume_recursive(
                &volume,
                volume_id,
                source,
                parent_hint,
                &mut entries,
                &mut total_bytes,
                &mut seen_inodes,
                state,
                events,
                operation_id,
                &tracker,
            )
            .await;
            emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
            result?;
        }
    }

    let file_count = entries.iter().filter(|e| !e.is_dir).count();
    let dirs_count = entries.iter().filter(|e| e.is_dir).count();

    // Emit final scan progress (scan complete: tallies == totals)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Scanning,
            None,
            file_count,
            file_count,
            total_bytes,
            total_bytes,
        )
        .with_scan_meta(None, dirs_count, None),
    );

    // Handle dry-run mode
    if config.dry_run {
        events.emit_dry_run_complete(DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: file_count,
            bytes_total: total_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        });
        return Ok(());
    }

    // Phase 2: Delete. Files first, then directories deepest-first.
    // entries are already in order: children before parents (due to recursive scan).
    // We process files first, then dirs in reverse order.
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Delete files: one at a time, or in the backend's own batches
    // (`Volume::delete_batch_size`, an object store's `DeleteObjects`). A batch
    // goes by key with no per-file stat: these are files the scan just listed.
    let files: Vec<&VolumeDeleteEntry> = entries.iter().filter(|e| !e.is_dir).collect();
    let batch_size = volume.delete_batch_size();
    for batch in files.chunks(batch_size.map_or(1, std::num::NonZeroUsize::get)) {
        // The cooperative boundary, between files (or batches) in the
        // destructive phase. The scan recursion above is NOT gated, so pausing
        // mid-enumeration can't freeze a half-counted "Scanning…".
        if state.stop_or_park_async().await {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(),
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // E2E throttle so cancel-during-delete tests on fast virtual MTP have a
        // deterministic window in which to click Cancel before all files are
        // gone. Reuses the copy throttle knob (`set_test_throttle` exposes a
        // generic per-step throttle). Zero-cost outside E2E.
        if let Some(pause) = state.scope().host().pacing().copy_throttle() {
            tokio::time::sleep(pause).await;
        }

        let results = match batch_size {
            Some(_) => delete_batch(&volume, state, batch).await,
            None => {
                let mut one = Vec::with_capacity(batch.len());
                for entry in batch {
                    one.push(delete_entry(&volume, state, &entry.path, entry.listed, None).await);
                }
                one
            }
        };

        // Every file the batch removed is journaled and counted, even when
        // another in the same batch failed; the FIRST failure then ends the
        // delete, reported against its own path.
        let mut first_failure: Option<WriteOperationError> = None;
        for (entry, result) in batch.iter().zip(results) {
            match result {
                Ok(()) => {}
                // Cancel landed mid-iteration (during the throttle sleep, an
                // outage wait, or inside delete_with_cancel itself). The
                // top-of-loop cancel check only catches cancels that land
                // between iterations; without this arm, the `?` propagation
                // would surface as a Cancelled error with no `write-cancelled`
                // event, breaking the settle contract (see the module
                // CLAUDE.md § "Settle contract").
                Err(EntryStop::Cancelled) => {
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Delete,
                        files_processed: files_done,
                        rollback: CancelRollback::none(),
                    });
                    return Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    });
                }
                Err(stop) => {
                    if first_failure.is_none() {
                        first_failure = Some(stop.into_error(&entry.path, volume.as_ref()));
                    }
                    continue;
                }
            }

            // Journal the deleted leaf under the REAL volume id so "when did I
            // delete dog.jpg" is searchable for SMB / MTP too. Delete is never
            // rollbackable (the op finalizes `permanent_delete`), so these rows
            // exist purely for search — one per leaf, deliberately.
            super::super::journal::record_volume_leaf(
                state.scope(),
                cmdr_operation_log::types::EntryType::File,
                volume_id,
                &entry.path,
                None,
                Some(entry.progress_bytes as i64),
                None,
                false,
                cmdr_operation_log::types::ItemOutcome::Done,
            );

            files_done += 1;
            // `progress_bytes` (not `size`) so the numerator stays in lockstep
            // with the dedup'd `total_bytes` denominator. See
            // `VolumeDeleteEntry::progress_bytes` for the contract.
            bytes_done += entry.progress_bytes;
        }
        if let Some(failure) = first_failure {
            return Err(failure);
        }

        if last_progress_time.elapsed() >= state.progress_interval
            && let Some(entry) = batch.last()
        {
            let current_file = entry.path.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Deleting,
                    current_file.clone(),
                    files_done,
                    file_count,
                    bytes_done,
                    total_bytes,
                ),
            );
            state.scope().status().update(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                file_count,
                bytes_done,
                total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (already in deepest-first order from scan_volume_recursive)
    for entry in entries.iter().filter(|e| e.is_dir) {
        // The cooperative boundary (delete phase, between dirs).
        if state.stop_or_park_async().await {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(),
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        match delete_entry(&volume, state, &entry.path, entry.listed, None).await {
            Ok(()) => {}
            Err(EntryStop::Cancelled) => {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rollback: CancelRollback::none(),
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }
            // Best effort, as it always was: a folder still holding something
            // the delete couldn't take refuses, and stays.
            Err(EntryStop::Failed { error, lost: None }) if error.transport_fault().is_none() => {}
            // An outage that ended it, or a folder someone else swapped for
            // something else meanwhile: reported, ❌ never swallowed into a
            // delete that reads as complete.
            Err(stop) => return Err(stop.into_error(&entry.path, volume.as_ref())),
        }
    }

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: files_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
        appeared_during_move: None,
        top_level_skipped: None,
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: None,
    });

    Ok(())
}

#[cfg(test)]
#[path = "volume_walker_tests.rs"]
mod tests;
