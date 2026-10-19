# Hunt: drive index, store side (`index-store`)

Scope: `crates/cmdr-index/` store half (writer, store, handle, lifecycle, read, aggregator, host, relocation, volume
files, retention), `apps/desktop/src-tauri/src/index_host.rs`, `apps/desktop/src-tauri/src/listing_index_sizes/`.

The store itself held up: every file it deletes is named through `volume_files` from hashed, sanitized volume ids, the
open path deletes only on a schema mismatch or `DatabaseCorrupt`/`NotADatabase`, relocation moves one file after a
`TRUNCATE` checkpoint, and every index consumer that drives a write (`expected_totals`, `search_leaves`) gates on
completeness. The one survivor is a consumer that DOESN'T gate: the folder size the conflict prompt shows when it asks
someone to replace a folder with a file.

Two findings: zero critical, zero high, one medium, one low.

### index-store-1. "Replace this folder with a file?" shows the index's lower-bound or stale folder size as an exact number

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/operations_host/index.rs:30`, `AppOperationIndex::recursive_size`; consumed at
  `crates/cmdr-operations/src/conflict.rs:356`, `resolve_conflict` (via `durability.rs:301`, `lookup_indexed_size`);
  rendered by `apps/desktop/src/lib/file-operations/transfer/TransferConflictDialog.svelte:106`.
- **Excerpt:**

```rust
30	    fn recursive_size(&self, path: &Path) -> Option<u64> {
31	        index()
32	            .dir_stats(&path.to_string_lossy())
33	            .ok()
34	            .flatten()
35	            .map(|s| s.recursive_size)
36	    }
```

and, in `crates/cmdr-index/src/indexing/read/queries.rs`, the flags that line 35 throws away:

```rust
421	fn dir_stats_from(path: String, s: &store::DirStatsById, current_epoch: u64, pending: Option<PendingView>) -> DirStats {
422	    let complete = s.min_subtree_epoch > 0;
...
432	        recursive_size_complete: complete,
433	        recursive_size_stale: complete && s.min_subtree_epoch < current_epoch,
```

- **Mechanism:**
  1. A local copy/move in Stop mode meets a clash where the source is a file and the destination is a real folder
     (`ClashKind` cross-type).
  2. `resolve_conflict` builds the destination `ClashSide` with `size_for_dir = lookup_indexed_size(dest_path)` →
     `AppOperationIndex::recursive_size` → `Index::dir_stats` → `get_dir_stats_on_volume`.
  3. `dir_stats` answers `Some` for ANY directory that has a `dir_stats` row, including one whose subtree isn't fully
     listed (`min_subtree_epoch == 0`, a lower bound) or was listed at an older epoch, and regardless of the volume's
     freshness (the read pool is installed for Stale volumes too). It carries `recursive_size_complete` /
     `recursive_size_stale` / `recursive_size_pending` to say so.
  4. `recursive_size` keeps only the `u64`. `build_conflict_event` puts it in `destination_size` and computes
     `size_difference` from it; `WriteConflictEvent` has no completeness field.
  5. The dialog renders `formatByteSize(destSize)` as a plain exact size next to the red "replace a folder with a file"
     warning. A plain `Overwrite` there is the one answer `resolution_for_clash` accepts as consent to drop the folder,
     and its "* all" carry latches for every later cross-type clash in the op.
- **Trigger:** any of these, all routine:
  - The destination drive is mid-first-index (phased first index leaves listed ancestors with `min_subtree_epoch == 0`
    for minutes), so a 40 GB folder whose depths aren't walked yet reads as "12 KB".
  - The folder has an unreadable subfolder (`listed_epoch = 0` + `unreadable_cause`), so it's permanently a lower bound.
  - The app just launched and the boot-disk replay or a `LocalExternal` drive (loads Stale) hasn't caught up with what
    changed while Cmdr was closed, or the writer is behind a big copy into that folder (the hourglass state, also
    dropped). The user copies `Project` (a file) onto a folder `Project`, sees "Folder, 12 KB, smaller than the incoming
    file", and clicks Overwrite (or Overwrite all).
- **Guard ruled out:** `expected_totals` and `search_leaves` both refuse incomplete subtrees (`min_subtree_epoch == 0`,
  and `search_leaves` also `< current` plus `is_fresh`); `recursive_size` has neither gate. The cross-type consent rule
  (`conflict.rs:250`) needs an explicit Overwrite but doesn't check what the prompt told the person. The FE's
  `(unknown)` fallback only triggers on `None`. The listing grid renders `≥`/stale for the same data
  (`measure-column-widths.ts:230`), but the conflict dialog gets no flags to do the same. No test covers a lower-bound
  destination size in the conflict event.
- **Impact:** a person consents to deleting a whole folder tree based on a size the index knew was a floor or out of
  date. The tree is replaced through the safe-overwrite path, so it's gone once the item lands (Rollback of a finished
  copy removes the copied file, not restores the folder; that's the oplog hunter's area to confirm). Recoverable only
  from backups.
- **Suggested fix:** have `OperationIndex::recursive_size` return `None` unless
  `recursive_size_complete && !recursive_size_stale && !recursive_size_pending` (mirroring `expected_totals`), or carry
  the completeness through `WriteConflictEvent` so the dialog renders `≥ 12 KB` / "size not known yet" for the folder
  being replaced.

### index-store-2. The delete confirmation lists a folder's index lower bound as its exact size

- **Severity:** low
- **Confidence:** plausible (the FE path from enriched `FileEntry` to `DeleteSourceItem` not traced line by line)
- **Where:** `apps/desktop/src/lib/file-operations/delete/DeleteDialog.svelte:328`, `itemSizeBytes`;
  `apps/desktop/src/lib/file-operations/delete/delete-dialog-utils.ts:13`, `DeleteSourceItem`.
- **Excerpt:**

```ts
322	    /** Formats item size for display. Folders show recursive info when available.
323	     *  Always uses logical (content) sizes (not worth plumbing the display mode setting
324	     *  through the delete dialog infrastructure for a transient confirmation dialog). */
325	    function itemSizeBytes(item: DeleteSourceItem): number | null {
326	        // Group A wire-format: IPC sends `null` for absent fields, not `undefined`.
327	        return item.isDirectory ? (item.recursiveSize ?? null) : (item.size ?? null)
328	    }
```

- **Mechanism:** `DeleteSourceItem` has `recursiveSize` / `recursiveFileCount` but no `recursiveSizeComplete` /
  `recursiveSizeStale`, so a per-row folder size the listing shows as `≥ 12 KB` (index lower bound) reaches the delete
  confirmation as a plain "12 KB, 3 files".
- **Trigger:** delete (Shift+F8, permanent) a folder on a drive mid-first-index or with an unreadable subfolder.
- **Guard ruled out:** the dialog also runs a real scan preview whose totals are walked, which is why this is low: the
  headline totals are honest, only the per-row annotation isn't.
- **Impact:** misleading per-item size in a permanent-delete prompt; no loss on its own.
- **Suggested fix:** carry `recursiveSizeComplete` into `DeleteSourceItem` and render the listing's `≥` / stale state,
  or drop the per-row index size once the scan preview has its own number.

## Near misses

- Volume id path escape into `index-{id}.db`: killed by `cmdr_fs::volume::ids` (every non-root id is
  `{scheme}-{slug}-{blake3}`, slug alphanumerics only; MTP/ADB serials go through the funnel).
- `IndexStore::open` wiping a good index: killed by typed classification (`indicates_corruption` is only
  `DatabaseCorrupt | NotADatabase`; busy retries; everything else keeps the file). Cmdr data only anyway.
- Relocation stranding a WAL or replaying a stray one: killed by `fold_the_wal_in` (TRUNCATE checkpoint + busy check +
  non-empty-WAL check) and `delete_sidecars(new_db)` before the rename.
- Dev and prod sharing a drive-index dir (the instance lock is on the DATA dir, the index is in the CACHE dir): killed
  by `config::cache_dir_from` (`<CMDR_DATA_DIR>/cache` fallback) and `tauri-wrapper.ts` setting `CMDR_CACHE_DIR` only
  when it composed the data dir. Residual: a shell that exports its own `CMDR_CACHE_DIR` to two differently-dataed
  instances would share one; not realistic.
- Retention evicting a live DB: `select_evictions` filters registered ids; the snapshot-then-remove window could race a
  volume registering, but the loss is a rebuildable cache.
- `search_leaves` writing a stale leaf set into the oplog: gated on `is_fresh` + `min_subtree_epoch == current`, and the
  rows are `search_only` (rollback uses the top-level unit).
- `expected_totals` feeding a free-space or overwrite decision: it's progress-denominator only and returns `None` for
  incomplete subtrees.
- On-demand folder size (`listing_index_sizes/count/measure.rs`) following symlinks out of the tree: `WalkDir` defaults
  to not following links; read-only anyway.
- Agent `list_dir` reading index children for a decision: results carry coverage/lower-bound fields, and agent tools
  can't write (only `propose_rename_plan`).

## Covered

Read in full (non-test parts):

- `crates/cmdr-index/src/volume_files.rs` (non-test half)
- `crates/cmdr-index/src/drive_index_relocation.rs`
- `crates/cmdr-index/src/indexing/store/connection.rs`
- `crates/cmdr-index/src/indexing/read/expected_totals.rs`
- `crates/cmdr-index/src/indexing/resources/retention.rs` (non-test half)
- `apps/desktop/src-tauri/src/index_host.rs`
- `apps/desktop/src-tauri/src/operations_host/index.rs`
- `crates/cmdr-operations/src/host/index.rs`
- `apps/desktop/src-tauri/src/listing_index_sizes/count/measure.rs`, `count/publish.rs`

Read in part: `indexing/lifecycle/state/teardown.rs` (`clear_index`, `finish_clearing`, `clear_every_index`),
`indexing/read/queries.rs` (`get_dir_stats*`, `dir_stats_from`), `indexing/handle/mod.rs` (the read methods),
`indexing/store/schema.rs` (`apply_pragmas`), `indexing/store/errors.rs` (`indicates_corruption`),
`indexing/store/mod.rs` (`DirStats`), `cmdr-fs/src/volume/ids.rs`, `apps/desktop/src-tauri/src/config.rs` (cache/data
dir), `crates/cmdr-operations/src/conflict.rs` (`resolution_for_clash`, `resolve_conflict` Stop arm,
`build_conflict_event`, `apply_resolution`), `TransferConflictDialog.svelte`, `DeleteDialog.svelte`.

## Not covered

- `indexing/writer/` (all of it: `mod.rs`, `entries.rs`, `delta.rs`, `aggregation.rs`, `maintenance.rs`, `repair/`).
- `indexing/aggregator/`.
- `indexing/store/entries.rs`, `dir_tree.rs`, `dir_stats.rs`, `meta.rs`, `paths.rs`.
- `indexing/lifecycle/` beyond the teardown clear path (`manager*`, `phases/`, `cover/`, `startup.rs`,
  `scan_completion.rs`, `freshness.rs`, `master.rs`).
- `indexing/read/enrichment.rs`, `coverage.rs`, `pending_sizes.rs`, `handles.rs`.
- `indexing/host/`, `indexing/deletes.rs`, `indexing/hold.rs`.
- `listing_index_sizes/mod.rs`, `refresh.rs`, `schedule.rs`, `touched.rs`, `count/mod.rs`, `count/jobs.rs`.

These are Cmdr's own cache state (at most medium per the brief); the hunt prioritized every path where index data
reaches a write or delete of user files, which is where both findings sit.
