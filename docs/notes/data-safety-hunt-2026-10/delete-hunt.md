# Data-safety hunt 2026-10: delete, trash, clipboard (hunter report)

Slug: `delete`. Four findings survive: one critical, two high, one medium.

### delete-1. Deleting a folder on an external drive, cloud drive, or Android phone also empties every folder a symlink inside it points at

- **Severity:** critical
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operations/src/delete/walker.rs:440`, `scan_volume_recursive`;
  `crates/cmdr-operations/src/delete/walker.rs:741`, the no-preview parent hint in
  `delete_volume_files_with_progress_inner`; `crates/cmdr-local/src/reading.rs:343`, `process_dir_entry`;
  `crates/cmdr-adb/src/volume/mapping.rs:31`, `with_link_target`; `crates/cmdr-local/src/local_posix/scan.rs:118`
  (cold-preview top-level stat)
- **Excerpt:**

```rust
// crates/cmdr-operations/src/delete/walker.rs:440
        for child in &children {
            let child_path = PathBuf::from(&child.path);
            if child.is_directory {
                Box::pin(scan_volume_recursive(
                    volume,
                    volume_id,
                    &child_path,
                    Some(true),
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
```

```rust
// crates/cmdr-local/src/reading.rs:345
    let is_symlink = file_type.is_symlink();

    let target_is_dir = if is_symlink {
        fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        false
    };
    ...
    let is_dir = metadata.is_dir() || target_is_dir;
```

- **Mechanism:**
  1. The pane's `volumeId` for anything off the boot volume is that volume's own id: every attached drive and cloud
     drive is a `LocalPosixVolume` registered under its discovery id
     (`apps/desktop/src-tauri/src/file_system/mod.rs:162`, `:175`), and `openDeleteDialog` passes the pane's id as
     `sourceVolumeId` (`apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:730`, `:756`).
  2. `delete_files_start` sends every id other than `"root"` to the volume walker
     (`crates/cmdr-operations/src/local_start.rs:398`).
  3. `scan_volume_recursive` lists each directory (`list_directory_with_cancel`, or the oracle's cached listing,
     line 411) and recurses into every child with `child.is_directory`. It never looks at `child.is_symlink`.
  4. `LocalPosixVolume`'s listing reports a symlink to a directory as `is_directory: true, is_symlink: true`
     (`reading.rs:360`), and so does ADB (`with_link_target` sets `is_directory = true`). The cached oracle listing
     carries the same `FileEntry`.
  5. The walker recurses into `folder/link`, which lists the TARGET's contents, and queues `folder/link/a.jpg`,
     `folder/link/sub/…` as files.
  6. Delete phase: `LocalPosixVolume::delete` (`crates/cmdr-local/src/local_posix.rs:571`) `lstat`s the leaf and calls
     `remove_file("folder/link/a.jpg")`, which resolves `link` in the middle of the path and unlinks the target's file.
     ADB runs `rm -f` on the same path, also resolved through the link. Then the target's subdirectories go too
     (`remove_dir` / `rmdir`), and finally the link itself.
  7. Top-level variant: a selected symlink-to-dir when the parent has no authoritative listing (cold preview).
     `LocalPosixVolume::scan_subtree` answers `top_level_is_directory` from `std::fs::metadata` (follows the link,
     `local_posix/scan.rs:118`), so the cache-hit arm `Some(_)` (walker.rs:649) recurses into the target. Without a
     preview, the oracle parent hint (walker.rs:741) also hands `is_directory = true` for the link. The dialog meanwhile
     shows "Only the link will be deleted, not its target" (`delete-dialog-utils.ts:83`).
- **Trigger:** Shift+F8 (or F8 on a volume with no trash, or an online-only Dropbox folder, which forces permanent) on a
  folder on a USB/Thunderbolt drive, a disk image, a cloud drive opened from the volume switcher, or an Android phone,
  where the folder holds a symlink to a directory elsewhere (a project with a `data -> ~/Photos` link, a backup tree
  with links, a `node_modules`-style link farm pointing outside the folder).
- **Guard ruled out:** The local (`"root"`) walker is safe: `walk_dir_recursive` `lstat`s and records a symlink as a
  file (`scan_walker.rs:241`), and `walk_cached_entries` checks `entry.is_directory && !entry.is_symlink`
  (`scan_walker.rs:380`). The volume SCAN PREVIEW also checks `!entry.is_symlink` (`scan_preview/volume.rs:371`) and
  walkdir doesn't follow inner links, so the dialog's count does NOT include the target's files: nothing on screen
  warns. `LocalPosixVolume::is_directory` uses `lstat`, but the walker only calls it when no hint exists; both hint
  sources follow the link. `safety_grid_tests.rs:60` notes that `InMemoryVolume` has no symlinks, and no delete test
  covers a symlinked directory on a volume path (`rg symlink crates/cmdr-operations/src/delete/` hits only trash tests).
- **Impact:** Permanent, silent loss of files OUTSIDE the selection (the whole tree under each link target, on any
  volume the link reaches, including the boot disk). Not recoverable: delete has no rollback and the journal rows record
  the link-spelled paths.
- **Cross-slice:** the same root cause is reported from the ADB side as `mtp-adb-1`; treat the two as one defect (this
  entry covers both `LocalPosixVolume` and ADB). Per hunt-mtp-adb, SFTP has the TOP-LEVEL variant only: its listing
  reports a link as a leaf (`crates/cmdr-sftp/src/volume/mapping.rs:40`), but `get_metadata_impl` follows
  (`crates/cmdr-sftp/src/volume/query.rs:86`), so a selected link to a folder answers `is_directory` true through the
  walker's unhinted probe; its `entry_kind_impl` (`query.rs:110`, an `lstat`) would answer `Symlink`. The fix shape
  below (ask `entry_kind` for the top level) covers it.
- **Suggested fix:** In `scan_volume_recursive`, recurse only into `child.is_directory && !child.is_symlink` and queue a
  symlink as a leaf (one `delete` of the link); make the cache-hit top-level arm and the parent hint treat `is_symlink`
  the same way (or resolve the top level with `entry_kind`, which is an `lstat`). Add a `LocalPosixVolume` delete test
  with an inner and a top-level symlinked directory.

### delete-2. F8 / Shift+F8 (and Cmd+X / Cmd+C) can act on a different file than the one selected when the folder changes at that moment

- **Severity:** high
- **Confidence:** plausible (the backend-applies-before-the-frontend window is documented, not traced through the diff
  pipeline)
- **Where:** `apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:694`, `openDeleteDialog`;
  `apps/desktop/src-tauri/src/commands/clipboard.rs:32`, `selected_paths` (used by `copy_files_to_clipboard` /
  `cut_files_to_clipboard`); `crates/cmdr-listing/src/engine/rows.rs:74`, `:86` (unguarded lookups)
- **Excerpt:**

```ts
// apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:691
    // Fetch full FileEntry data in a single batch IPC call
    let entries
    try {
      entries = await getFilesAtIndices(listingId, backendIndices, access.getShowHiddenFiles())
    } catch (error) {
```

```ts
// apps/desktop/src/lib/file-explorer/pane/transfer-operations.ts:89 (the guard F5/F6 has)
let snapshot: { paths: string[]; fileCount: number; folderCount: number }
try {
  snapshot = await getSelectionSnapshot(listingId, context.showHiddenFiles, backendIndices, expectedSequence)
} catch (error) {
  if (!asListingLookupError(error)) throw error
  // Refuse rather than reinterpret old selection indices under a newer revision.
  addToast(tString('fileExplorer.compareDirectories.keptChanging'), { level: 'warn' })
  return null
}
```

- **Mechanism:**
  1. The frontend's selection and cursor are row indices into the pane's APPLIED revision (`pane/CLAUDE.md`: "Row
     indices belong to `pane-row-state.ts`'s applied revision").
  2. A watcher insert or remove updates the backend listing cache first; the frontend applies the diff later.
  3. In that window, `openDeleteDialog` sends the old indices to `getFilesAtIndices`, which reads whatever rows sit at
     those indices NOW (`files_at_indices` has no sequence parameter). A row inserted above the selection shifts every
     index by one.
  4. The dialog's `sourceItems` and `sourcePaths` both come from those wrong entries, so it lists, scans, and deletes
     the neighbor. `cut_files_to_clipboard` does the same through `paths_at_indices`, so the next paste MOVES the
     neighbor.
  5. F5/F6 and copy-path closed exactly this hole with `getSelectionSnapshot(…, expectedSequence)`; `pane/DETAILS.md` §
     Copy-path says why: "a watcher insert renumbers rows, and the old indices would name other files."
- **Trigger:** Pressing Shift+F8 or F8 in a folder something else is writing to (`~/Downloads` during a browser
  download's `.crdownload` rename, a build output folder, a log folder, a synced cloud folder), with the selection below
  the changing row. With "skip confirmation" on, an F8 trash runs with no dialog at all.
- **Guard ruled out:** The dialog shows names, but only the first 10 (`MAX_VISIBLE_ITEMS`), and a person who just
  selected the rows rarely re-reads them. `delete_files_start`'s `expected_sources` binding is the MCP caller's, `None`
  for a person. The preview binding (`take_cached_scan_result`) only checks that the preview and the operation name the
  same paths, which they do (both are the wrong paths).
- **Impact:** A permanent delete of a file the user never selected (unrecoverable), or a trash or a move of the wrong
  file (recoverable).
- **Suggested fix:** Resolve the delete dialog's entries and the clipboard copy/cut paths through the revision-checked
  `getSelectionSnapshot` / `selection_names` path with the pane's applied sequence, refusing on `changed` the way
  `buildTransferPropsFromSelection` does; then `files_at_indices` / `paths_at_indices` have no destructive caller left.

### delete-3. A permanent delete of a folder that contains a mount point wipes the mounted volume's contents

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operations/src/scan_walker.rs:215`, `walk_dir_recursive`;
  `crates/cmdr-operations/src/delete/walker.rs:142`, `delete_files_with_progress_inner`;
  `crates/cmdr-operations/src/validation.rs:36`, `validate_sources`
- **Excerpt:**

```rust
// crates/cmdr-operations/src/scan_walker.rs:273
    } else if metadata.is_dir() {
        if is_symlink_loop(path, visited) {
            return Err((ctx.on_symlink_loop)(path));
        }

        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        dirs.push(path.to_path_buf());
        ...
        } else {
            let entries = fs::read_dir(path).map_err(|e| (ctx.on_io_error)(path, e))?;
            for entry in entries.flatten() {
                if is_left_out(ctx, path, device, &entry.file_name()) {
                    continue;
                }
                walk_dir_recursive(
```

- **Mechanism:**
  1. `walk_dir_recursive` reads `metadata.dev()` (line 236) but uses it only for the AppleDouble sidecar rule
     (`is_left_out`). A directory on another device is walked like any other.
  2. A mount point inside the selection (an sshfs / macFUSE mount, a `mount_smbfs` or `mount_nfs` into a custom folder,
     a disk image attached with `-mountpoint`, a pCloud-style FUSE drive) is a plain directory to `lstat`, so the walk
     descends into the mounted filesystem and collects every file there.
  3. `delete_files_with_progress_inner` unlinks each one (`fs::remove_file`, line 142), then `remove_dir` on the mount
     point fails (busy), silently (`let _ =`).
  4. The same holds for selecting the mount point itself in `/Volumes` while browsing the boot volume: the whole drive
     is emptied.
- **Trigger:** Shift+F8 on a folder like `~/mnt` or `~/dev/old-project` that has a share or image mounted somewhere
  inside it, or on a drive's folder in `/Volumes`. F8 refuses (the OS won't trash a busy mount), and the obvious next
  step is the permanent delete.
- **Guard ruled out:** `validate_sources` checks existence only. `readOnlyRefusal('delete')` covers read-only volumes.
  Nothing in `scan_walker.rs` or `delete/` compares `st_dev` against the source's. The dialog's recursive count includes
  the mount's files, which is the only signal, and it doesn't say they are on another volume. Finder refuses to delete a
  folder holding a mounted volume; `rm -rf` crosses it, `rm -rx` doesn't.
- **Impact:** Permanent loss of a whole mounted volume's contents (often a NAS or a remote server, so possibly other
  people's data), which the user didn't think of as part of the folder.
- **Suggested fix:** Have the delete walk stop at a device boundary: refuse the delete up front with a typed error
  naming the mount (preferred, matching Finder), or skip the mounted subtree and report it as left behind.

### delete-4. Undoing a trash can move back a different folder that later took the trashed one's name in the Trash

- **Severity:** medium
- **Confidence:** plausible (the Trash reusing a free name is standard macOS behavior, not verified here)
- **Where:** `crates/cmdr-operation-log/src/rollback.rs:672`, `restore_move`;
  `apps/desktop/src/lib/file-operations/delete/trash-undo.ts:220`, `runTrashUndo` / `undoLastTrash`
- **Excerpt:**

```rust
// crates/cmdr-operation-log/src/rollback.rs:708
    // For a file, verify it hasn't changed since the op (dirs: existence only —
    // a subtree isn't cheaply verifiable, so existence + a clear target is the
    // contract). Drift / unverifiable ⇒ skip.
    if unit.entry_type == EntryType::File {
        match verify_snapshot(unit.size, unit.mtime, from_entry.size, from_entry.modified_at) {
            SnapshotVerdict::Match => {}
            SnapshotVerdict::Drift => return ItemResult::Skipped(SkipReason::Drift),
            SnapshotVerdict::Unverifiable => return ItemResult::Skipped(SkipReason::UnverifiablePrecondition),
        }
    }
```

- **Mechanism:**
  1. Cmdr trashes `~/Work/Invoices/` and journals its in-trash location, `~/.Trash/Invoices`.
  2. That item leaves the Trash (emptied, Finder's Put Back then moved on, or deleted from the Trash), and later a
     different folder named `Invoices` is trashed from anywhere (Finder, Cmdr, another app). The Trash gives it the free
     name `~/.Trash/Invoices`.
  3. Edit > Undo move to trash (kept for the whole session) or the operation log's undo runs `restore_move`: the
     recorded `from` exists, it's a `Dir` row so only existence is checked, and `~/Work/Invoices` is free, so the
     stranger folder is renamed into `~/Work/`.
- **Trigger:** Trash a folder in Cmdr, empty the Trash, trash another same-named folder, then use Edit > Undo move to
  trash (or the operation log's undo) for the first one.
- **Guard ruled out:** The restore-target occupancy check (`rollback.rs:722`) protects the destination, not the identity
  of the source. Files are guarded by the size/mtime snapshot; directories have none. `is_self_collision` doesn't apply
  (different folded paths).
- **Impact:** A folder the user meant to throw away reappears in an unrelated place, reported as "restored". No bytes
  are lost, but the item vanishes from where the user expects it (the Trash), and the toast credits a restore that
  didn't happen.
- **Suggested fix:** Record the trashed directory's `(dev, ino)` (or its `mtime`/birth time) on the `rollback_unit` row
  and require it to match `from_entry` before a `Dir` restore, skipping as `Drift` otherwise.

## Near misses

- **Trash undo onto an existing name** (incl. case-only): guarded by `restore_move`'s occupant check plus
  `AlreadyExists` from the no-replace rename; a case-different occupant is found by the case-insensitive lookup and
  isn't a self-collision (folded paths differ).
- **Cut, failed or partial paste, later paste deletes sources that never landed**: the cut flag is cleared at paste
  START (`clipboard-operations.ts:481`), so a retry is a copy; and the move engine only removes a source after it lands.
- **Volume with no trash falling to a permanent delete silently**: `supportsTrash=false` forces the permanent mode WITH
  a banner; `skippedDeleteConfirmation` refuses to skip in that case; the backend trash never converts a refusal into a
  delete (`trash.rs` only reports `TrashRefused`).
- **Stale delete preview (paths changed since the dialog opened)**: `take_cached_scan_result` binds on the path set;
  changed leaves fail `remove_file` or are removed as what they now are, inside the selection. A dir swapped for a
  symlink between preview and confirm could be followed by `remove_file(dir/child)`, but that needs a hostile actor.
- **Files added to a folder after its preview**: the local walker deletes only the previewed files and `remove_dir`
  fails silently, so the delete reports success with the folder still there. Wrong result, no loss.
- **Online-only routing**: decided per item from `lstat` flags, all-or-nothing, never on the refusal; the leaf link
  isn't followed (`resolve_through_symlinks`).
- **SMB share with server-side symlinks**: the server presents a followed link as a plain directory (cmdr-smb never sets
  `is_symlink`), so a volume delete recurses into it, but only inside the share and exactly as Finder over SMB would.
- **Archive routing on the first source only** (`local_start.rs:382`): a mixed batch from one pane can't happen; a
  non-archive path reaching the archive driver or an archive path reaching the local walker fails rather than deleting.
- **Paste-as-file collision**: `create_numbered_file` retries on the typed `AlreadyExists` from an exclusive create, so
  it never overwrites.
- **Cut state reused after a Finder copy of the same paths**: the clipboard set matches the cut set, so a paste moves
  instead of copies. Surprising, but the files are moved, not lost.

## Covered

- `crates/cmdr-operations/src/delete/walker.rs`
- `crates/cmdr-operations/src/delete/trash.rs`
- `crates/cmdr-operations/src/delete/cloud_trash.rs`
- `crates/cmdr-operations/src/delete/volume_start.rs`
- `crates/cmdr-operations/src/delete/mod.rs`
- `crates/cmdr-operations/src/delete/CLAUDE.md`
- `apps/desktop/src-tauri/src/clipboard/mod.rs`, `state.rs`, `store.rs`, `paste_file.rs`, `pasteboard.rs`, `CLAUDE.md`
- `apps/desktop/src-tauri/src/commands/clipboard.rs` (lines 1–260)
- `apps/desktop/src/lib/file-operations/delete/DeleteDialog.svelte`, `delete-scan-state.svelte.ts`,
  `delete-dialog-utils.ts`, `trash-undo.ts`, `go-to-trash.ts`, `CLAUDE.md`
- Followed out of the slice (relevant parts only): `crates/cmdr-operations/src/local_start.rs` (delete/trash start),
  `scan_walker.rs` (walk + cached walk), `scan_cache_previews.rs` (`take_cached_scan_result`), `scan_preview/volume.rs`
  (oracle and cold preview), `validation.rs` (`validate_sources`), `create_numbered.rs`;
  `crates/cmdr-local/src/reading.rs` (`process_dir_entry`, `get_single_entry`), `local_posix.rs` (`is_directory`,
  `delete`), `local_posix/scan.rs`; `crates/cmdr-adb/src/volume/{mapping,query,writes}.rs` (symlink mapping, delete);
  `crates/cmdr-operation-log/src/rollback.rs` (`restore_move`, `is_self_collision`);
  `crates/cmdr-operations/src/rollback.rs` (`InverseAct::Restore`);
  `apps/desktop/src/lib/file-explorer/pane/{file-operation-commands,dialog-state.svelte,clipboard-operations,transfer-operations,confirmation-skip,trash-availability}.ts`
  (delete and paste entry points); `apps/desktop/src-tauri/src/file_system/mod.rs` (volume registration)

## Not covered

- `apps/desktop/src-tauri/src/clipboard/payload.rs` and `mock.rs` (content-paste flavor picking and the E2E mock; the
  write itself was checked through `create_numbered.rs`).
- `apps/desktop/src/lib/file-operations/delete/TrashCompleteToastContent.svelte` and `delete/DETAILS.md` (backend
  `delete/DETAILS.md` only grepped).
- `commands/clipboard.rs` lines 260–428 (content paste command and Linux stubs).
- Volume eject (`file_system/volume/drive_release/`, `volumes/unmount_approver/`, `server_move_smb.rs`): not read; a
  busy set and a DiskArbitration dissenter exist, so it looked guarded at a glance.
- The MTP backend's delete (path-to-handle resolution with duplicate names) and the SMB backend's listing were not read
  in full.
