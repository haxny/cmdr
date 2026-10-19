# Data-safety hunt 2026-10: delete, trash, clipboard (verifier report)

Slug: `delete`. Verified against the worktree at `22918588b`. All four excerpts are verbatim and current.

### delete-1

- **Verdict:** confirmed
- **Severity:** critical (agree)
- **Evidence:**
  - `crates/cmdr-operations/src/delete/walker.rs:440-455`: `scan_volume_recursive` recurses on `child.is_directory`
    alone; there's no `is_symlink` read anywhere in `delete/walker.rs`
    (`rg is_symlink crates/cmdr-operations/src/delete/` hits only `cloud_trash.rs:217`). Both child sources (the
    oracle's `authoritative_listing` at `:411` and `list_directory_with_cancel`) carry the backend's `FileEntry`
    unchanged.
  - Delete phase (`walker.rs` ~`:830-865`) only filters `!e.is_dir` and calls `volume.delete` / `delete_files`; no guard
    there either. The trait default `delete_files` (`crates/cmdr-fs/src/volume/mod.rs:702`) loops `delete`, so no
    backend default saves it.
  - Routing: `local_start.rs:398` sends every id other than `"root"` to `start_volume_delete`. A pane gets a non-root id
    when it's opened on a drive or cloud drive from the switcher, a favorite, or startup (`resolve_path_volume_fast`,
    `apps/desktop/src-tauri/src/volumes/mod.rs:243`, matches cloud roots first, then the mount point);
    `openDeleteDialog` passes that id (`file-operation-commands.ts:729`). A root pane that merely browses into
    `/Volumes/X` keeps `"root"` and is safe.
  - Per backend, what a symlink-to-dir looks like and whether the walker descends:
    - **LocalPosixVolume (attached drives, disk images, cloud drives, every mount from `register_every_mount`)**:
      `is_directory: true, is_symlink: true` (`crates/cmdr-local/src/reading.rs:345-356`). Descends.
      `LocalPosixVolume::delete` (`local_posix.rs:571`) `lstat`s the LEAF only, so `remove_file("dir/link/a.jpg")`
      resolves `link` mid-path and unlinks the target's file; the target's subdirs then go through `remove_dir`; the
      link itself is finally `remove_file`d (its `lstat` isn't a dir). **Affected.**
    - **ADB**: `with_link_target` sets `is_directory = true` (`crates/cmdr-adb/src/volume/mapping.rs:31`, applied in
      `query.rs:110`). Descends; `rm -f` runs on link-spelled paths. The final entry for the link itself hits `probe`
      (`writes.rs:250`), which FOLLOWS links, so it's treated as a dir, `rmdir` on a symlink fails, the re-list is
      empty, and the op ends in an error AFTER the target's contents are gone. **Affected**, but realistic exposure is
      small: Android shared storage (FUSE/sdcardfs) rarely holds user-made links; the big one is `/sdcard` itself.
    - **SFTP**: listing maps `RawFileType::Symlink` to `is_directory: false`
      (`crates/cmdr-sftp/src/volume/mapping.rs:40-42`), no link folding in `query.rs`. Walker queues the link as a leaf;
      `delete_impl` (`mutation.rs:149`) `SSH_FXP_REMOVE`s the link. **Safe.**
    - **SMB**: `FileEntry::new(..., is_directory, false)` (`crates/cmdr-smb/src/volume/mapping.rs:30`, `query.rs:201`):
      never a symlink. A server-followed link looks like a real dir and is descended, but only inside the share (the
      hunter's near miss; agree).
    - **MTP, S3, WebDAV**: no symlink concept (`cmdr-mtp/.../volume_impl.rs:145`, `cmdr-s3/src/volume/query.rs:216-247`,
      `cmdr-webdav/src/volume/mapping.rs:15` all pass `false`). **Not affected.**
  - Top-level variant holds too. Warm preview: the oracle branch of `scan_preview/volume.rs:371` treats a top-level link
    as a file (`top_level_is_directory: false`), so the delete's cache-hit arm queues it as a leaf: safe. Cold preview:
    `LocalPosixVolume::scan_subtree` sets `top_level_is_directory` from `std::fs::metadata` (follows,
    `local_posix/scan.rs:118`), so the `Some(_)` arm (`walker.rs:649`) recurses into the target. No-preview:
    `parent_hint` (`walker.rs:731-745`) returns the cached `e.is_directory`, which is `true` for the link.
  - Precedent for the fix exists in-tree: the index's network walker refuses to descend a link
    (`crates/cmdr-index/src/indexing/network_scanner/mod.rs:289`, `walk_descends` at `:343`).
  - No test covers a symlinked dir on the volume delete path (`delete/` symlink hits are trash tests and a
    `symlink_metadata` call in `preview_binding_tests.rs`).
- **Why critical stands:** permanent, silent loss outside the selection, in an ordinary flow (Shift+F8 on a project
  folder on an external SSD, or any delete on a volume without trash). The preview count excludes the target's files, so
  nothing on screen hints at it.

### delete-2

- **Verdict:** plausible (mechanism confirmed; the race window is real but only milliseconds wide, not measured)
- **Severity:** high (agree, at the low end)
- **Evidence:**
  - `openDeleteDialog` resolves indices with `getFilesAtIndices` (`file-operation-commands.ts:694`), which has no
    sequence parameter (`commands/file_system/listing.rs:368-373`, `cmdr-listing/src/engine/rows.rs:86`).
    `copy_files_to_clipboard` / `cut_files_to_clipboard` go through `selected_paths` → `paths_at_indices`
    (`commands/clipboard.rs:32-45`), also unchecked.
  - F5/F6 use `getSelectionSnapshot(…, expectedSequence)` and refuse on `changed` (`transfer-operations.ts:89-91`);
    copy-path does the same (`selected-paths-read.ts:66`). The pane's own docs name the hazard: "a watcher insert
    renumbers rows, and the old indices would name other files" (`pane/DETAILS.md:913-916`).
  - The window: the backend cache moves first, then the FE reconciles selection synchronously when the diff event lands
    (`listing-diff-sync.svelte.ts:150-180`). So the gap is IPC latency plus any time the JS thread is busy, small per
    keypress but hit regularly in a churning folder.
  - Why high and not lower: a Shift+F8 always shows the dialog, but with a multi-row selection every index shifts by
    one, so one neighbor is deleted and one selected file survives, and the first-10 name list makes that easy to miss.
    Trash and cut-then-paste are recoverable; the permanent delete isn't.
  - Extra site the hunter didn't list: `start_selection_drag` (`commands/file_system/drag.rs:108-122`) resolves through
    `paths_at_indices` too, so a drag-to-move or drag-to-Trash has the same race. The fix should cover it.

### delete-3

- **Verdict:** confirmed
- **Severity:** high (agree)
- **Evidence:**
  - `walk_dir_recursive` reads `metadata.dev()` (`scan_walker.rs:236-238`) and passes `device` only to `is_left_out`
    (AppleDouble sidecars, `:296`) and `walk_cached_entries`; it never compares it with the source root's device.
    `rg 'st_dev|\.dev\(\)|mount'` over `scan_walker.rs`, `delete/walker.rs`, and `validation.rs` finds only
    `validation.rs:381` / `:474`, which are copy/move same-file and same-device checks, not delete.
  - No mount-point refusal anywhere on the delete path: `is_mount_point` is used only by `transfer_sides.rs:261-265`;
    nothing in `delete/`, `local_start.rs`, or `file-operation-commands.ts`.
  - Delete phase: `fs::remove_file(&file_info.path)` per file (`delete/walker.rs:142`), then directories; a busy mount
    point's `remove_dir` fails after its contents are gone.
  - The volume walker has the same gap (it never asks for a device at all), so a nested mount under a LocalPosixVolume
    pane is hit as well.
  - Why high and not critical: the dialog's recursive count includes the mounted files (the one signal), and the trigger
    (a mount nested inside a folder, or selecting a `/Volumes` entry from a root pane and choosing Shift+F8) is less
    common. The loss is total and permanent when it happens, and it can be other people's data on a NAS.

### delete-4

- **Verdict:** plausible (Trash reusing a freed name is standard macOS behavior; not exercised here)
- **Severity:** medium (agree)
- **Evidence:**
  - `restore_move` (`crates/cmdr-operation-log/src/rollback.rs:672-733`) checks the recorded in-trash `dest_path`
    exists, verifies size+mtime for `EntryType::File` only (`:711-717`), and for a dir only checks the restore target is
    free (`:725-732`). No identity (inode, birth time) is recorded or compared for a dir.
  - The in-trash path is the real `resultingItemURL` (`delete/trash.rs:59-68`), so a later item that lands on the same
    freed name matches.
  - Reachability: Edit > Undo move to trash only offers the newest un-undone Cmdr trash this session
    (`trash-undo.ts:53`), so the stranger must be trashed by Finder or another app, or the user must undo an older op
    from the operation log. That narrows it, but it's reachable.
  - No bytes are lost; a folder the user threw away reappears at the old location and the result says "restored". Medium
    fits.

## Summary

- delete-1: confirmed, critical. LocalPosix volumes (drives, images, cloud drives) and ADB are affected; SFTP, SMB
  (beyond in-share), MTP, S3, and WebDAV are not.
- delete-2: plausible, high. Also covers `start_selection_drag`.
- delete-3: confirmed, high.
- delete-4: plausible, medium.
