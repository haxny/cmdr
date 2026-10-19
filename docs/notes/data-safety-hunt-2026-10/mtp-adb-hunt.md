# Data-safety hunt 2026-10: MTP and ADB (`mtp-adb`)

Hunter's report for the phone backends: `crates/cmdr-mtp/`, `apps/desktop/src-tauri/src/mtp/`, `crates/cmdr-adb/`,
`apps/desktop/src-tauri/src/adb/`, and `device_volumes.rs`. Five findings survived, ranked. Read-only pass at
`22918588b`.

### mtp-adb-1. Deleting a folder that holds a symlink on an ADB phone deletes what the link points at

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operations/src/delete/walker.rs:350`, `:442`, `scan_volume_recursive`;
  `crates/cmdr-adb/src/volume/mapping.rs:31`, `with_link_target`; `crates/cmdr-adb/src/volume/query.rs:104`,
  `list_directory_impl`; `crates/cmdr-adb/src/volume/writes.rs:136`, `delete_impl`
- **Excerpt:**

```rust
// crates/cmdr-adb/src/volume/mapping.rs:31
pub(super) fn with_link_target(mut entry: FileEntry, target: &SyncStat) -> FileEntry {
    if target.kind() == SyncEntryKind::Directory {
        entry.is_directory = true;
        entry.size = None;
    }
    entry
}
```

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
```

- **Mechanism:**
  1. `AdbVolume::list_directory_impl` stats every symlink's target and folds it in with `with_link_target`, so a link to
     a folder comes back as `is_directory: true, is_symlink: true`. `get_metadata_impl` does the same for a single path.
  2. The volume delete walker never reads `is_symlink`. A child with `is_directory` recurses (`walker.rs:442`); an
     unhinted top-level source asks `volume.is_directory(path)` (`walker.rs:353`), whose trait default is
     `get_metadata().is_directory`, so a selected link reads as a folder too. The trait itself says not to do this
     (`crates/cmdr-fs/src/volume/mod.rs:326`, `entry_kind`: "❌ never `is_directory` before ... recursively deleting a
     path").
  3. The recursion lists `<dir>/link`. ADB's `LIST` of a link lists its target, and each child path is built under the
     link (`adb://s/…/link/photo.jpg`).
  4. Phase 2 deletes leaves first: `delete_impl` probes `…/link/photo.jpg` (a file) and runs
     `rm -f '/…/link/photo.jpg'`. The kernel resolves the intermediate `link` component, so the TARGET's file goes.
     Subfolders go by `rmdir` through the link the same way.
  5. Last, `delete(link)`: `probe` follows the link and says `Directory`, `rmdir` on a symlink fails with `ENOTDIR`, the
     follow-up listing is now empty, so the verb's `IoError` is returned. The link survives; everything it pointed at is
     gone.
- **Trigger:** a developer (the audience ADB is for) deletes a folder under `/data/local/tmp` that contains a symlink to
  a working tree elsewhere, for example `/sdcard/Projects`. The preview counts the link as ONE entry (`scan_walk.rs:284`
  checks `!entry.is_symlink`), so the dialog shows a small count while the walk empties the target. Selecting the link
  itself does it too, as does selecting `/sdcard` at the device root (the shell user can `rm` everything under shared
  storage).
- **Guard ruled out:** the preview cache doesn't help (it only supplies the top-level hint, and the copy walk reports a
  top-level link as a directory, `scan_walk.rs:93`). No `authoritative_listing` on ADB (`can_watch_listings` is false),
  so the walker always lists live. `delete_impl` is strictly single-node, but it's handed paths THROUGH the link, so
  single-node doesn't stop it. The move source sweep is safe (`source_sweep.rs` asks `entry_kind` and deletes a carried
  link as the link); only the delete walker is exposed.
- **Impact:** permanent deletion of files outside the selection; delete has no rollback.
- **Suggested fix:** have the volume walker treat `is_symlink` as a leaf (or ask `Volume::entry_kind` where it now asks
  `is_directory`), and make `AdbVolume::delete_impl` remove a symlink with `rm -f` rather than following it to `rmdir`.
  Same root cause as `hunt-delete`'s delete-1 (which also covers `LocalPosixVolume`). SFTP has the TOP-LEVEL half only:
  its listing reports a link as a link (`crates/cmdr-sftp/src/volume/mapping.rs:40`, readdir attrs), so a nested link is
  a leaf, but `get_metadata_impl` uses a following `stat` (`crates/cmdr-sftp/src/volume/query.rs:86`), so a selected
  link to a folder answers `is_directory` and the walker lists and deletes the target through it. Its own
  `entry_kind_impl` (`query.rs:110`) would answer `Symlink` if the walker asked it.

### mtp-adb-2. An MTP delete, rename, or move acts on whatever object now holds a cached handle, without checking its name

- **Severity:** high
- **Confidence:** plausible (handle reuse on a given device not traced; the code's own docs assert it)
- **Where:** `crates/cmdr-mtp/src/connection/mutation_ops.rs:100`, `delete_object_with_cancel`; `:330`, `rename_object`;
  `:416`, `move_object`; `crates/cmdr-mtp/src/connection/resolve.rs:90`, `resolve_path_to_handle`;
  `crates/cmdr-mtp/src/connection/event_loop.rs:145`
- **Excerpt:**

```rust
// crates/cmdr-mtp/src/connection/mutation_ops.rs:100
        let object_handle = self.resolve_path_to_handle(device_id, storage_id, object_path).await?;
        let device_arc = self.device_arc(device_id).await?;

        let device = acquire_device_lock(&device_arc, device_id, "delete_object").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| self.map_device_error(e, device_id))?;

        // Get object info to check if it's a directory
        let object_info = storage
            .get_object_info(object_handle)
            .await
            .map_err(|e| self.map_device_error(e, device_id))?;

        let is_dir = object_info.is_folder();
```

- **Mechanism:**
  1. `resolve_path_to_handle` answers from `PathHandleCache.path_to_handle` whenever the path is there, with no age and
     no check against the device (`resolve.rs:90`).
  2. Nothing ever removes a forward entry except Cmdr's own mutations: `finalize_listing` only INSERTS
     (`directory_ops.rs:698`), so a name missing from a fresh listing keeps its old handle; `ObjectRemoved` refreshes
     panes and the index but leaves the path cache alone (`event_loop.rs:145`).
  3. `delete_object_with_cancel` fetches `object_info` for that handle (it needs `is_folder`) and deletes it.
     `object_info.filename` and `object_info.parent` are in hand and never compared with `object_path`. `rename_object`
     and `move_object` have the same shape.
  4. The crate's docs say handles go stale and get reused: "Android routes MTP through MediaProvider, whose object
     handles are NOT stable across a media rescan" (`connection/DETAILS.md` § "Stale parent handle on upload", with a
     field report) and "MTP devices reuse object handles" (§ "Keeping both directions in lockstep"). A stale handle that
     the device has since given to another object makes the delete land on that object.
- **Trigger:** a camera or phone that doesn't send events for its own changes (the code notes "many cameras don't",
  `volume_impl.rs:200`) keeps a pane's stale listing on screen, and `listing_watch_coverage` answers `EveryWriter`, so
  nothing re-lists. The user deletes a photo on the device, takes new ones, then deletes a stale row in Cmdr: the cached
  handle now names a new photo, and that's the one deleted. Same for a rename or a move-off-the-phone's source delete.
- **Guard ruled out:** a session reset drops the whole cache (`session_reset.rs`), but nothing else does. An invalid
  handle is refused by the device, which is the common Android case, so the hole needs REUSE, not just staleness. The
  delete walker lists folders live (fresh handles), so in-folder children are safe; the exposure is top-level selected
  files, where the preview's `authoritative_listing` hit skips any listing.
- **Impact:** permanent deletion (or rename or move) of an object the user never selected.
- **Suggested fix:** before any destructive call, compare `object_info.filename` (and `parent`) with the path's last
  component and parent handle; on a mismatch drop the forward entry, re-list the parent, and resolve again (or refuse).
  Also prune a parent's stale forward entries in `finalize_listing`.

### mtp-adb-3. Two same-named files in one MTP folder collapse to one: every action on either row hits the last-listed twin

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-mtp/src/connection/directory_ops.rs:880`, `entry_for_object`; `:697`, `finalize_listing`;
  `crates/cmdr-mtp/src/connection/cache.rs:37`, `PathHandleCache::insert`
- **Excerpt:**

```rust
// crates/cmdr-mtp/src/connection/directory_ops.rs:692
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                for (path, handle) in cache_updates {
                    storage_cache.insert(path, handle);
                }
            }
        }
```

- **Mechanism:** PTP allows two siblings with one name (`crates/cmdr-mtp/DETAILS.md` § "The no-clobber rename is
  check-then-act"). The listing returns both as rows with the same `path`; `insert` keys on the path, so the second
  overwrites the first and every path-addressed op (`open_read_session`, `delete_object_with_cancel`, `rename_object`)
  resolves both rows to the LAST-listed object. A copy off the phone of both rows reads the second twin twice (the
  conflict dialog then offers "keep both" for what is the same bytes) and never reads the first. A delete of the first
  row deletes the second.
- **Trigger:** a camera, or a device that complies with a colliding `SendObjectInfo` / rename, holding duplicate names;
  the user copies the folder to the Mac as a backup, then wipes the card. One of each pair was never copied.
- **Guard ruled out:** a MOVE is safe: `SourceStamp` carries the handle as `inode`, so the twin that wasn't copied
  doesn't match the ledger and stays (`source_sweep.rs:161`). A copy has no such check, and the copy's success report
  counts both rows.
- **Impact:** a backup silently missing files (lost once the user clears the device); a delete of the wrong twin.
- **Suggested fix:** detect same-name siblings in `convert_object_infos` / `list_directory_for_scan` and either refuse
  path-addressed ops on an ambiguous name (typed error) or disambiguate the row's path (carry the handle) so each row
  resolves to its own object.

### mtp-adb-4. Overwriting a folder by rename on an ADB phone moves the item INTO the folder, replacing a same-named file inside

- **Severity:** medium
- **Confidence:** confirmed (traced end to end; toybox `mv` into-directory semantics from its POSIX contract, not run on
  a device)
- **Where:** `crates/cmdr-adb/src/volume/writes.rs:160`, `rename_impl`; reached from
  `apps/desktop/src/lib/file-explorer/pane/rename-flow.svelte.ts` `handleConflictResolve` (`overwrite-delete`) through
  `crates/cmdr-operations/src/rename.rs:405`
- **Excerpt:**

```rust
// crates/cmdr-adb/src/volume/writes.rs:160
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let device_from = self.to_device_path(from)?;
        let device_to = self.to_device_path(to)?;
        debug!("AdbVolume::rename: {device_from} → {device_to}, force={force}");
        if !force && self.probe(&device_to).await != WhatIsThere::Nothing {
            return Err(VolumeError::AlreadyExists(device_to));
        }
        if self.probe(&device_from).await == WhatIsThere::Nothing {
            return Err(volume_error_from_errno(ENOENT, &device_from));
        }
        self.shell_verb(&["mv", "-f", &device_from, &device_to], &device_to)
            .await?;
        self.notify_renamed(from, to).await;
        Ok(())
    }
```

- **Mechanism:** the rename editor's conflict dialog offers "overwrite" for any taken name, folders included; ADB has no
  trash, so it's `overwrite-delete` → `rename(from, to, force = true)`. `rename_target` lets a taken name through when
  `force` (`rename.rs:405`). `mv -f a b` with `b` an existing directory (or a link to one) doesn't replace `b`, it moves
  `a` to `b/a`, and `-f` silently replaces a `b/a` that's already there. `notify_renamed` then patches the pane as if
  `b` had been replaced.
- **Trigger:** in `/sdcard/Download`, rename `notes.txt` to `Backup` where a `Backup` folder already holds a
  `notes.txt`, and confirm the overwrite.
- **Guard ruled out:** a local `rename(2)` refuses file-over-folder (`EISDIR`) and folder-over-non-empty-folder
  (`ENOTEMPTY`); nothing on the ADB path re-checks the destination's kind with `force`, and the transfer engine's own
  landings pass `force = false`, so only the rename dialog reaches it.
- **Impact:** `Backup/notes.txt` is replaced without being named; the renamed item vanishes into a folder; the pane
  shows a state that isn't on the device. Partly recoverable (the moved item is findable), the replaced file is not.
- **Suggested fix:** with `force`, refuse (or clear first, the way the dialog promises) when the destination is a
  directory; use `mv -fT` where toybox has it, or stat and refuse a kind mismatch before the verb.

### mtp-adb-5. An ADB no-clobber check reads "couldn't ask" as "nothing there", so a transient sync failure lets `mv -f` clobber

- **Severity:** low
- **Confidence:** confirmed (traced end to end; the transient failure itself is the unusual condition)
- **Where:** `crates/cmdr-adb/src/volume/writes.rs:250`, `probe`; callers `writes.rs:164` (`rename_impl`,
  `force = false`) and `crates/cmdr-adb/src/volume/streams.rs:190` (`write_from_stream_impl`, `WriteMode::CreateNew`)
- **Excerpt:**

```rust
// crates/cmdr-adb/src/volume/writes.rs:250
    pub(super) async fn probe(&self, device: &str) -> WhatIsThere {
        let Ok(mut session) = self.open_sync(device).await else {
            return WhatIsThere::Nothing;
        };
        let what = match session.stat(device).await {
            Ok(stat) if stat.exists() => match stat.kind() {
                SyncEntryKind::Directory => WhatIsThere::Directory,
                SyncEntryKind::Symlink => match self.follow(&mut session, device).await {
                    Some(target) if target.kind() == SyncEntryKind::Directory => WhatIsThere::Directory,
                    _ => WhatIsThere::NotADirectory,
                },
                _ => WhatIsThere::NotADirectory,
            },
            _ => WhatIsThere::Nothing,
        };
        session.quit().await;
        what
    }
```

- **Mechanism:** a refused socket, a `Timeout`, a protocol hiccup, or a v1 `STAT` that can't stat for any reason
  (mode 0) all become `Nothing`. `rename_impl(force = false)` and the `CreateNew` landing then run `mv -f`, which
  replaces the occupant. The `Volume::rename` contract calls `force == false` "the only thing standing between a move
  and the file it would replace".
- **Trigger:** a flaky cable or a busy `adbd` during a bulk rename or a copy's landing, at the instant of the probe.
- **Guard ruled out:** for `rename_impl` the second `probe(from)` must then succeed, which narrows it; the `CreateNew`
  landing has no second probe. The documented TOCTOU window (`writes.rs` § "The two accepted TOCTOU windows") is a
  different, accepted race: this one is a misread answer, and costs nothing to fix.
- **Impact:** a destination file replaced that the caller was told would be refused.
- **Suggested fix:** make the guard probes return `Result<WhatIsThere, VolumeError>` and refuse on an error (keep the
  infallible form for the classifier, where "can't tell" is fine).

## Near misses

- ADB shell injection: `shell::quote` single-quotes every argument correctly (`'` → `'\''`), and every device path
  starts with `/`, so no name can turn into an option or a second command.
- ADB `..` escapes: `paths.rs::climbs_above_the_root` refuses a climb above `/` before `RemoteRoot` absorbs it.
- ADB write atomicity: every `write_from_stream` stages on `<name>.cmdr-tmp-<uuid>` and lands by `mv -f`; a failed pump
  removes the staging name, and the original is untouched until the rename.
- ADB `copy_within`'s cancel `rm -f device_to` only ever hits the engine's staged temp (the trait contract makes the
  caller stage it).
- ADB on case-insensitive `/sdcard`: a landing onto a case variant is refused because `probe` stats through the
  filesystem's own folding.
- MTP overwrite: no rename-over exists, and the engine never deletes first; it sets the original aside with a
  `force = false` rename to a `.cmdr-temp-` name (`displaced_destination.rs`), which the transfer reaper doesn't match.
- MTP upload failure / unplug mid-write: `upload_from_stream` deletes `upload_err.partial`; the engine writes to a
  staging name, so even an old Android that `O_TRUNC`s on a reused name never sees the user's name.
- MTP `force = true` rename onto a taken name duplicates instead of replacing on a permissive device, and Android
  refuses it; neither destroys data (the engine never relies on it, per `displaced_destination.rs`).
- MTP delete recursion: `MtpVolume::delete` passes `SingleNode`, and the listing for the emptiness check is live.
- MTP move source sweep with duplicate names: `SourceStamp` includes the handle, so an uncopied twin stays (see
  mtp-adb-3).
- MTP cross-folder rename with a new name is two calls (`move_object`, then `rename_object`); a failing second step
  leaves the item at the new folder under its old name and reports the error. Wrong place, nothing lost.
- ADB unplug mid-op: `on_unmount` stops new sync sockets; in-flight shell verbs fail with the device; no partial wears a
  user name.
- `device_volumes.rs`: `device_volume_for_path` matches on a `/` boundary, so `adb://R58M1` doesn't claim
  `adb://R58M10/…`.

## Handed off

- To `hunt-archive`: `crates/cmdr-operations/src/archive_edit/remote.rs::swap_into_place` on MTP deletes the original
  before `rename(remote_temp, archive_path, false)`. A failed rename leaves the only copy under a `.cmdr-tmp-` name.
  Reported there as archive-1: the user isn't told, and `cleanup::reap_stale_transfer_temps` deletes the temp on the
  next copy into that folder. (The leftover does show in panes: staging hides a temp only while its op holds the guard.)
- To `hunt-delete`: the walker half of mtp-adb-1.

## Covered

- `crates/cmdr-adb/CLAUDE.md`, `crates/cmdr-adb/DETAILS.md`
- `crates/cmdr-adb/src/shell.rs`, `src/volume/writes.rs`, `src/volume/mutation.rs`, `src/volume/paths.rs`,
  `src/volume/volume_impl.rs`, `src/volume/streams.rs`, `src/volume/mod.rs`, `src/volume/query.rs`,
  `src/volume/mapping.rs`, `src/volume/scan.rs`
- `crates/cmdr-mtp/CLAUDE.md`, `crates/cmdr-mtp/DETAILS.md`, `src/connection/CLAUDE.md`, `src/connection/DETAILS.md`
  (relevant sections)
- `crates/cmdr-mtp/src/connection/mutation_ops.rs`, `file_ops.rs`, `resolve.rs`, `cache.rs`, `directory_ops.rs`,
  `event_loop.rs` (event dispatch, lines 120–280)
- `crates/cmdr-mtp/src/volume/volume_impl.rs`, `scan.rs`, `mapping.rs`, `streams.rs`, `mod.rs` (`to_mtp_path`)
- `apps/desktop/src-tauri/src/adb/CLAUDE.md`, `adb/commands.rs`, `adb/volume_wiring.rs`
- `apps/desktop/src-tauri/src/mtp/CLAUDE.md`
- `apps/desktop/src-tauri/src/device_volumes.rs`
- Followed out of the slice: `crates/cmdr-operations/src/delete/walker.rs` (scan + delete phases),
  `crates/cmdr-fs/src/volume/scan_walk.rs` (top-level + inner walk), `crates/cmdr-fs/src/volume/mod.rs` (`rename`,
  `is_directory`, `entry_kind`, `write_from_stream`, `copy_within` contracts), `crates/cmdr-operations/src/rename.rs`
  (`rename_target`, the volume branch), `rename-flow.svelte.ts` (conflict resolution), `transfer/staged_write.rs`
  (force-rename sites), `transfer/volume/displaced_destination.rs`, `transfer/volume/source_sweep.rs`,
  `archive_edit/remote.rs` (`swap_into_place`).

## Not covered

- `crates/cmdr-adb/src/{transport,sync,devices,server,features,errors,params,fuzzing}.rs` and
  `src/volume/{state,index_scope}.rs`: the wire codecs and lifecycle, read only through their callers.
- `crates/cmdr-mtp/src/connection/{mod,session_reset,handle_resolver,bulk_ops,scheduler,errors,events,dates,usb_owner,volume_registrar}.rs`,
  the rest of `event_loop.rs`, `virtual_device.rs`, `discovery.rs`, `types.rs`, `volume/{cancel,read_bench}.rs`.
- `apps/desktop/src-tauri/src/adb/device_provider.rs`, `mtp/{watcher,volume_wiring,events,macos_workaround,mod}.rs`.
- `mtp-rs` itself (out of scope).
