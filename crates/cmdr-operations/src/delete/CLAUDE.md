# Delete + trash

A local-FS walker (`walkdir` + `fs::remove_file`), a volume walker (every backend), and OS-native trash.

`../../CLAUDE.md` holds the shared `WriteOperationState`, `OperationIntent`, cancel, ETA, and settle contracts;
`../transfer/CLAUDE.md` is the copy + move parallel. Frontend counterpart:
`apps/desktop/src/lib/file-operations/delete/CLAUDE.md`.

## Files

- **`walker.rs`** (local) and **`volume_walker.rs`** (any `Volume`, its scan in `volume_scan.rs`), routed by
  `../local_start.rs`. The volume scan asks `authoritative_listing` before every `list_directory`; deletes batch per
  `delete_batch_size`. DETAILS § "Volume-delete internals".
- **`outage.rs`**: waits out a dropped share. ❗ A re-run re-checks the entry (kind, size); a mismatch stops the delete
  (`ChangedWhileDisconnected`); `NotFound` means deleted ONLY after a wait. ❌ No `through_outages` around a delete.
  DETAILS § "Waiting out a dropped share".
- **`trash.rs`**: `move_to_trash_sync()` (macOS `trashItemAtURL`; Linux `trash` crate; reused by
  `trash_single_journaled`), `trash_files_with_progress()` (batch, per-item progress, cancel), and
  `trash_dir_for_path()` (❌ keep its ancestor walk, DETAILS § Where a trash is). Refusals are a typed `MutationError`,
  ❌ never a sentence; every item it can't take emits its own `Failed` source-item event. Existence checks use
  `symlink_metadata()`.
- **`cloud_trash.rs`**: `routing_for_selection()` behind the `trash_routing_for_paths` command, plus `is_online_only`
  (the one `SF_DATALESS` read). DETAILS § "A trash of online-only cloud content becomes a delete".
- **`volume_start.rs`**: a volume delete's managed lifecycle (here, not `../local_start.rs`, because its body is
  `async`). DETAILS § "The volume delete's own lifecycle".

## Must-knows

- **Files first, then directories deepest-first**, so `remove_dir` always finds an empty one. **Not rollbackable**:
  Cancel stops further deletes.
- **Both walkers reuse the scan-preview cache via `config.preview_id`** and emit identical events; on a hit an initial
  `phase: Deleting` event keeps the FE's denominator right.
- **A `preview_id` alone doesn't authorize acting on a path set.** The LOCAL walker iterates `scan_result.files` and
  never re-reads its `sources`, so an unbound cache deletes the PREVIEWED tree, with no rollback. Bind with
  `take_cached_scan_result`; ❌ never skip it.
- **A symlink is a LEAF to the volume walker**: descend only an `EntryKind::Directory` (`EntryKind::of`,
  `Volume::entry_kind`), ❌ never `is_directory` or a preview's word (both follow links). DETAILS § "A link is a leaf".
- **A delete reaching a mount point refuses WHOLE, up front** (`MountedVolumeInside`), in both walkers and the delete
  preview. DETAILS § "A mount is a wall".
- **❌ Never resolve a top-level source's type with `.unwrap_or(false)`.** Hand the `Option` to `scan_volume_recursive`,
  which propagates a failed probe: a guessed "file" books zero bytes for a whole tree.
- **Trash has no scan phase**: `trashItemAtURL` is atomic per top-level item, so progress tracks items. A PARTLY refused
  batch still COMPLETES, and its event carries `refused` (count + `strongest_refusal`): ❌ never `None`, or the ending
  reads as a clean success.
- **ONLINE-ONLY content (`SF_DATALESS`) in `~/Library/CloudStorage/<domain>/` trashes as a permanent delete**: trashing
  an evicted file DOWNLOADS it first. ❌ Never route on the folder alone (an ordinary Dropbox file trashes fine; routing
  it is data loss), ❌ never on the refusal (two `NSError` codes, non-deterministic), ❌ never on "no trash here", ❌
  not a volume question; only a provider we know by name, all-or-nothing. A FOLDER is answered by the scan preview's
  walk (`OnlineOnlyWatch`), ❌ no second walk. DETAILS.
- **A refusal carries a typed `TrashRefusalKind`, read from the `NSError` DOMAIN + CODE**, ❌ never its localized words
  (`error-string-match` forbids it). A failed batch is `WriteOperationError::TrashRefused`, ❌ not an `IoError` (one
  flattened sentence leaves the dialog only "try again"), reporting `strongest_refusal`, ❌ not the most common one.
- **Delete and trash don't `fsync`, and ❌ never a global `sync(2)`** (it stalled every other app; `clippy.toml`
  disallows `libc::sync`).
- **A recursive scan that bails with `Err(Cancelled)` must NOT emit `write-cancelled`; its top-level caller does**
  (`emit_cancelled_if_aborted`), or it fires once per stacked frame.

Depth: `DETAILS.md`.
