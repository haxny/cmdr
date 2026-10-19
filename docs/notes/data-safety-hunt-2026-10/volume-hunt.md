# Data-safety hunt 2026-10: volume layer and local FS (`volume`)

Hunter report. Scope: `crates/cmdr-fs/` (the `Volume` trait, write primitives, staging names, path anchoring, metadata
appliers), `crates/cmdr-local/`, `apps/desktop/src-tauri/src/file_system/volume/` (registry, roots, routing), and
`volume_host.rs`.

Result: the write primitives held up. `O_EXCL` creates, the one no-replace rename with its three-way degrade,
`sync_data` plus parent fsync, `ChildName`, the remote path grammar, and the registry's root set each have the guard the
focus areas asked about. One finding survived, and it's in the routing seam: the `.git` portal shadows a real directory
that only the rename path doesn't refuse.

### volume-1. F2 on the `worktrees` row in a repo's `.git` pane renames git's real `.git/worktrees`, breaking every linked worktree

- **Severity:** medium
- **Confidence:** plausible (backend traced end to end; the frontend's per-row gate traced to "pane-level only", not
  run)
- **Where:** `crates/cmdr-operations/src/rename.rs:131`, `rename_managed_inner` (only the archive route is refused);
  `crates/cmdr-git/src/virtual_listing.rs:40`, `list_categories` (the overlay row's path IS the real dir's path);
  `apps/desktop/src-tauri/src/commands/file_system/write_ops.rs:109`, `reject_if_routed_over_a_parent` (the guard
  copy/move/delete/trash get and rename doesn't);
  `apps/desktop/src/lib/file-explorer/pane/file-operation-commands.ts:137` (`readOnlyRefusal` judges the PANE's kind)
- **Excerpt:**

```rust
// crates/cmdr-operations/src/rename.rs:148
    if manager.path_is_inside_archive(&volume_id, &from).await || manager.path_is_inside_archive(&volume_id, &to).await
    {
        return (
            route_archive_rename(engine, &from, &to, &volume_id).await,
            InstantTarget::Archive,
        );
    }

    let is_root = volume_id == "root";
```

```rust
// crates/cmdr-git/src/virtual_listing.rs:40
pub fn list_categories(handle: &RepoHandle, repo_root: &Path) -> Vec<FileEntry> {
    let dot_git = repo_root.join(".git");
    Cat::ALL
        .into_iter()
        .map(|cat| {
            let segment = cat.as_segment();
            let path = dot_git.join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.to_string(), path, true, false);
            fe.permissions = 0o755;
```

- **Mechanism:**
  1. With the portal on, a repo's `.git/` listing gets six overlay rows (`overlay.rs::extra_entries`), and per
     `apps/desktop/src-tauri/src/file_system/git/DETAILS.md:148` a real entry whose name collides gives way to the
     virtual row: `.git/worktrees/` in a linked-worktree setup, and the legacy `.git/branches/`.
  2. The `.git/` pane itself isn't a routed path (`portal_route(".../.git")` is `None`), so `capabilitiesForPane` gives
     it the drive's writable kind, and `readOnlyRefusal('rename')` lets F2 (and click-to-rename) open on any row.
     `rename-flow.svelte.ts:221` runs `checkPermission` on `entry.path`, which stats the REAL `.git/worktrees` and finds
     it writable.
  3. The commit reaches `rename_managed_inner` with `from = <repo>/.git/worktrees`. It refuses only archive paths; there
     is no `portal_serves` check (the agent's planner has one, `agent/tools/propose/rename/plan.rs:398`; this path
     doesn't).
  4. `volume_id == "root"` → `rename_no_replace(<repo>/.git/worktrees, <repo>/.git/<new>)` succeeds on the real
     directory. A non-root local drive takes `LocalPosixVolume::rename` (plain `get`, not `resolve`) to the same end.
- **Trigger:** a person browsing `.git/` with the portal on, in a repo that has linked worktrees (David's own layout),
  presses F2 on the `worktrees` row or slow-double-clicks it, and commits a name, thinking they're renaming a view.
  Multi-rename (⌃M) over that row reaches `rename/bulk.rs` the same way.
- **Guard ruled out:** `reject_if_routed_over_a_parent` covers copy/move/delete/trash only. The routed `GitPortalVolume`
  is read-only, but rename uses `manager.get`, never `resolve`, so it never meets it. The frontend's git-portal refusal
  keys on the pane path, and the row's own path is never asked about. `checkPermission` answers for the shadowed real
  dir.
- **Impact:** every linked worktree's `.git` gitlink now points at a missing admin dir, so git in each worktree fails
  ("not a git repository"). The main repo stops counting those worktrees' HEADs and indexes as reachability roots, so a
  later `git gc` (after `gc.pruneExpire`, two weeks by default) can drop staged-but-uncommitted blobs and detached-HEAD
  commits that lived only there. Recoverable until then: the rename is journaled (undo), or a person renames it back.
  The legacy `.git/branches` collision is harmless (git stopped reading it).
- **Suggested fix:** refuse in `rename_managed_inner` (and the bulk path) when `portal_serves(from)` or
  `portal_serves(to)`, with the same typed `ReadOnly` refusal the other write commands give; on the frontend, gate F2
  per row with `rowIsOsVisible`-style `isVirtualGitPath(row.path)`.

## Covered

Read in full:

- `crates/cmdr-local/src/local_posix.rs`, `local_posix/streams.rs`, `local_posix/scan.rs`, `CLAUDE.md`
- `crates/cmdr-fs/src/volume/mod.rs` (trait defaults 360–760, `root_anchored`), `volume/child_name.rs`,
  `volume/retirement.rs`, `volume/ids.rs`, `volume/mkdir_all.rs`, `volume/secret_store.rs`,
  `volume/host/credentials.rs`, `volume/remote_paths.rs` (1–140), `volume/types.rs` (`WriteMode`), `staging.rs`,
  `staging/names.rs`, `durable.rs`, `apple_double.rs`, `validation.rs`, `metadata/blockers.rs`, `metadata/staged.rs`,
  `metadata/flags.rs` (read/write raw), `CLAUDE.md`
- `apps/desktop/src-tauri/src/file_system/volume/manager.rs`, `manager/roots.rs`, `manager/routing.rs`,
  `manager/archive_routing.rs`, `manager/git_routing.rs`, `manager/root_move.rs`, `mount_registration.rs`,
  `eject/escalate.rs`, `CLAUDE.md`, `DETAILS.md` (§ Path handling, § Key decisions through § Gotchas)
- `apps/desktop/src-tauri/src/volume_host.rs`
- Followed out of the slice: `crates/cmdr-operations/src/rename.rs` (100–300), `transfer/staged_write.rs` (200–275),
  `transfer/volume/cleanup.rs` (reaper), `transfer/volume/landed_flags.rs`, `transfer/volume/folder_metadata.rs`
  (stamping), `transfer_sides.rs` (mount-table gate), `validation.rs::ensure_destination_dir`,
  `native_drag/fulfillment.rs` (`CreateOrReplace` caller), `commands/file_system/write_ops.rs` (routed guards),
  `file_system/git/wiring.rs`, `git/overlay.rs`, `server_move_smb.rs` (eject-and-save), frontend
  `file-operation-commands.ts`, `rename-flow.svelte.ts`, `volume-capabilities.ts` (row gates)

## Not covered

- `crates/cmdr-local/src/reading.rs`, `mutation.rs`, `space.rs`, `live_space.rs`, `mount_table.rs`, `linux_mounts.rs`,
  `filesystem_kind.rs`, `statfs.rs`, `nsurl.rs`, `macos_metadata.rs` (read-only probes; skimmed by name only)
- `crates/cmdr-fs/src/volume/in_memory*` and `conformance*` (test oracle), `metadata/carried.rs`, `metadata/mod.rs`
  beyond `apply_landed`'s mode writer, `sqlite_util.rs`, `firmlinks.rs`, `file_provider.rs`, `cloud_provider.rs`,
  `tcc_paths.rs`, `process_memory/`, `icons/`, `deadline/`, `volume/host/` other than credentials, `scan_walk.rs`,
  `patching.rs`, `canonical_root.rs`, `name_fold.rs`, `smb_mount_source.rs`
- `apps/desktop/src-tauri/src/file_system/volume/eject/` other than `escalate.rs`, `drive_release/`,
  `manager/root_replace.rs`, `reconnect_error.rs`, `backends/`

## Near misses

- `rename_exclusive_with`'s check-then-rename on `ENOTSUP`/`EINVAL`/`ENOSYS`: racy by design, but `NameState::Unknown`
  refuses any stat that didn't prove "free"; `.`/`..` targets hit `EINVAL`, degrade, and stat as Occupied.
- `write_from_stream` `CreateOrReplace` truncates in place and cleans up only on cancel: every caller hands it a
  `.cmdr-tmp-*` temp or Finder's own placeholder (`staged_write.rs::write_mode`, `fulfillment.rs`).
- `write_from_stream`'s `create_dir_all(parent)` and the trait-default `create_directory_all` could recreate a vanished
  mount point and write to the disk underneath: `/Volumes` and `/run/media/$USER` refuse it, and a move's source delete
  waits on the mount-table gate (`transfer_sides::root_is_listed`).
- `root_anchored` doesn't resolve `..` for a local root: only person-typed paths reach it; listed names join through
  `ChildName`, and remote roots normalize before the component check (`remote_paths.rs`).
- Volume ID reuse after retirement: local IDs key on filesystem UUID; a `path-` ID reused by a new disk at the same
  mount point only shares the index cache and saved paths. In-flight temp records name UUID files, so an arrival sweep
  on the wrong disk matches nothing.
- `archive_volume_id` keys on the canonical zip path alone: remote app paths carry `user@host:port`/device prefixes, and
  the parsed-index cache keys on `(path, size, mtime)`, so a replaced zip re-parses.
- `apply_metadata_at` follows a link for mode and dates: it's called only on folders the op created and files it just
  landed by rename, never on a symlink.
- `clear_blockers`/`open_parent_folder`: `lchflags`, `SYMLINK_ACL`, and an lstat that refuses a linked parent; the
  parent's ACL/lock is restored on `Drop`.
- `cleanup.rs` stale-temp reaper deletes by substring `.cmdr-tmp-` plus a one-hour age: a `.cmdr-tmp-` file is never the
  only copy of data (the source survives until landing succeeds). A person's own file named that way would go, but
  that's hygiene, and it lives in the transfer engine (first hunt's slice).
- `escalate::force_volume` re-reads the volume's ACTIVE root after the normal eject timed out, so a promotion in between
  would force a sibling: every sibling of an old-address SMB ID is the same dead server, and a refusal never forces.
- AppleDouble `._name` skipped from a FAT/exFAT walk: the kernel serves it as `name`'s xattrs, which the copy carries;
  an orphan or a non-FAT `._` file still copies.
- `StoredCredentials` has no `Debug`, so a secret can't reach a log through `{:?}`.
