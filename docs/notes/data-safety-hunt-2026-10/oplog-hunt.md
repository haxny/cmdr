# Data-safety hunt 2026-10: operation log and rollback (`oplog`)

Slice: `crates/cmdr-operation-log/`, `apps/desktop/src-tauri/src/operation_log_host/`, and the rollback execution side
in `crates/cmdr-operations/src/` (`rollback.rs`, `reversal.rs`, `journal.rs`, `ledger.rs`), following calls into the
record points and the volumes' `rename`.

Six findings survive, ranked: two high, three medium, one low.

### oplog-1. Undoing a case-only rename on a case-sensitive volume overwrites the file now holding the old name

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operation-log/src/rollback.rs:284`, `is_self_collision`;
  `crates/cmdr-operation-log/src/rollback.rs:727`, `restore_move`; `crates/cmdr-local/src/reading.rs:316` (inode
  populated for hardlinks only); `crates/cmdr-local/src/local_posix.rs:610`, `LocalPosixVolume::rename`;
  `crates/cmdr-sftp/src/volume/mutation.rs:193`, `rename_impl` (forced rename replaces)
- **Excerpt:**

```rust
284	fn is_self_collision(same_volume: bool, from: &Path, to: &Path, from_entry: &FileEntry, occupant: &FileEntry) -> bool {
285	    if let (Some(a), Some(b)) = (from_entry.inode, occupant.inode) {
286	        return a == b;
287	    }
288	    same_volume && fold_path(from) == fold_path(to)
289	}
...
727	    let same_volume = from_vol_id == to_vol_id;
728	    let mut force = false;
729	    if let Ok(occupant) = to_volume.get_metadata(to_path).await {
730	        if is_self_collision(same_volume, from_path, to_path, &from_entry, &occupant) {
731	            force = true;
732	        } else {
733	            return ItemResult::Skipped(SkipReason::RestoreTargetOccupied);
734	        }
735	    }
```

```rust
316	    let inode = if metadata.is_file() && metadata.nlink() > 1 {
317	        Some(metadata.ino())
318	    } else {
319	        None
320	    };
```

- **Mechanism:**
  1. A rename (single or batch) of `IMG_0001.JPG` → `img_0001.jpg` journals one `rollback_unit` row, source
     `IMG_0001.JPG`, dest `img_0001.jpg`, same volume.
  2. Later a different file appears at `IMG_0001.JPG` (the volume is case-sensitive, so both names coexist).
  3. Undo: `restore_move` finds `from` unchanged (snapshot matches), then stats `to_path` and gets the NEW file as
     occupant.
  4. `is_self_collision`: `LocalPosixVolume` only sets `inode` for files with `nlink > 1`, and SFTP / S3 / WebDAV / SMB
     never set it, so both are `None` and it falls to the path fold. `fold_path` lowercases and NFC-normalizes, so the
     two paths fold equal ⇒ "self" ⇒ `force = true`.
  5. `ReversalRunner::perform` → `from.rename(from_path, to_path, true)`; `LocalPosixVolume::rename` with `force` is
     plain `std::fs::rename`, which replaces the target. SFTP's forced rename uses `posix-rename` or removes the target
     first.
  6. The new `IMG_0001.JPG` is destroyed and the item counts as reversed.
- **Trigger:** on a case-sensitive volume (SFTP to any Linux box, an S3 bucket, a case-sensitive APFS external drive, a
  Samba share with `case sensitive = yes`, or Linux local disk), the user renames with a case-only change (very common
  in a batch rename: "lowercase extensions"), a new file later lands at the old spelling (a camera re-import, a
  `git pull`, a sync client), and the user undoes the rename from history or via Ask Cmdr. The same happens for an
  NFD/NFC-only difference on a normalization-sensitive filesystem, and for a move between two sibling folders whose
  names differ only by case.
- **Guard ruled out:** the occupant check is the guard, and the fold is what defeats it. The inode branch can't save it:
  the local reader leaves `inode` `None` for ordinary files (the comment at `rollback.rs:275` assumes `LocalPosixVolume`
  always populates it, and `DETAILS.md` § "The two data-safety guards" says the same), and no remote backend sets it.
  The `same_volume` gate only handles cross-volume. `decision_tests.rs:81` pins the fold-means-self behavior;
  `DETAILS.md:742` notes the InMemory fixtures are case-sensitive but no cell tests a case-sensitive occupant.
- **Impact:** permanent, silent loss of the newer file (overwritten in place, not trashed). Not recoverable from Cmdr.
- **Suggested fix:** decide self-collision by identity only: compare the occupant against `from_entry` with a real node
  id (stat `(dev, ino)` for every local entry, not just hardlinks), and where no identity exists, ask whether the volume
  is case-insensitive AND confirm the occupant's listing name is exactly the `from` entry's (re-list the parent: on a
  case-insensitive volume only one entry exists). Otherwise skip as `RestoreTargetOccupied`.

### oplog-2. The journal interns folder paths case-insensitively, so on a case-sensitive volume a rollback acts on a sibling folder the operation never touched

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operation-log/src/store/migrations.rs:159` (`dirs_identity` on `name_folded`);
  `crates/cmdr-operation-log/src/store/mod.rs:334`, `intern_one`; `crates/cmdr-operation-log/src/store/mod.rs:364`,
  `reconstruct_dir_path`; consumed by `crates/cmdr-operation-log/src/rollback.rs:598`, `remove_file_if_unchanged`
- **Excerpt:**

```rust
334	fn intern_one(
335	    conn: &Connection,
336	    volume_id: &str,
337	    parent: Option<i64>,
338	    name: &str,
339	) -> Result<i64, OperationLogStoreError> {
340	    let folded = fold_name(name);
341	    conn.prepare_cached(
342	        "INSERT INTO dirs (volume_id, parent_dir_id, name, name_folded)
343	         VALUES (?1, ?2, ?3, ?4)
344	         ON CONFLICT (volume_id, IFNULL(parent_dir_id, 0), name_folded) DO NOTHING",
345	    )?
346	    .execute(rusqlite::params![volume_id, parent, name, folded])?;
347
348	    // `parent.unwrap_or(0)` mirrors the index's IFNULL(parent, 0): dir ids start
349	    // at 1, so 0 is a safe stand-in for a NULL (root) parent.
350	    let id = conn
351	        .prepare_cached(
352	            "SELECT dir_id FROM dirs
353	             WHERE volume_id = ?1 AND IFNULL(parent_dir_id, 0) = ?2 AND name_folded = ?3",
354	        )?
```

- **Mechanism:**
  1. Any earlier op on the volume interns `/srv/Media` (stored `name = "Media"`). Interning is forever (shared across
     ops until GC).
  2. A copy (or move) to `/srv/media/report.pdf` (a DIFFERENT folder on a case-sensitive volume) interns `/srv/media`:
     the `ON CONFLICT … name_folded` hits the `Media` row and the SELECT returns its id. The item's `dest_dir_id` now
     points at `/srv/Media`.
  3. Rollback: `map_rollback_unit` → `reconstruct_dir_path` rebuilds `/srv/Media/report.pdf` from the stored names.
  4. `remove_file_if_unchanged` stats that path. If absent ⇒ `AlreadyGone` ⇒ counted as reversed, the real copy in
     `/srv/media` stays, and history says "rolled back". If present with the recorded size (a volume leaf records size
     only, `journal.rs:257`; a local leaf size + preserved source mtime, so a duplicate of the same file matches) ⇒ it
     deletes the file in `/srv/Media`, which the op never wrote.
  5. A restore-move reads its `from` and `to` through the same folded dirs, so it can pull the wrong file back or put
     the right one into the wrong folder.
- **Trigger:** two sibling folders whose names differ only by case (or by Unicode normalization) on a case-sensitive
  volume: common on Linux servers over SFTP, S3 prefixes, and Samba shares; possible on a case-sensitive APFS drive. The
  user copies into one and rolls the copy back.
- **Guard ruled out:** the snapshot recheck runs against the wrong path, so it protects only when the sibling holds
  nothing same-named of the same size. The comment in `store/tests.rs:287` ("matching how the filesystem treats it")
  holds for the default macOS volume only; `fold_name` has no per-volume case sensitivity input.
- **Impact:** either a silent false "rolled back" (the copies stay, the op is now `rolled_back` and can't be retried),
  or permanent deletion of a same-named, same-size file in a folder the op never touched.
- **Suggested fix:** make `dirs_identity` exact on `name` (keep `name_folded` as a search column only), or include the
  volume's case sensitivity in the identity; a new migration would have to split existing merged rows, so at minimum
  stop merging new ones.

### oplog-3. "Finish rollback" never re-attempts a skipped item, then marks the whole op rolled back

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operation-log/src/store/mod.rs:619`, `read_rollback_units_page`;
  `crates/cmdr-operation-log/src/rollback/bookkeeping.rs:49`, `RunAcc::record`;
  `crates/cmdr-operation-log/src/rollback.rs:299`, `resolve_final_state`; the promise in
  `apps/desktop/src/lib/operation-log/operation-log-labels.ts:120`
- **Excerpt:**

```rust
625	    let sql = format!(
626	        "SELECT {ITEM_COLUMNS} FROM operation_items \
627	         WHERE op_id = ?1 AND row_role = ?2 AND outcome = ?3 AND seq < ?4 \
628	         ORDER BY seq DESC LIMIT ?5"
629	    );
630	    let mut stmt = conn.prepare_cached(&sql)?;
631	    let mut rows = stmt.query(rusqlite::params![
632	        op_id,
633	        RowRole::RollbackUnit.as_token(),
634	        ItemOutcome::Done.as_token(),
```

- **Mechanism:**
  1. First rollback: an item hits `RestoreTargetOccupied` (or `Drift`, `Locked`, `Failed`, `DirNotEmpty`).
     `RunAcc::record` writes the ORIGINAL item's outcome to `skipped` via `set_item_outcomes`; the op lands
     `partially_rolled_back`.
  2. The user clears the blocker (moves the occupant aside, unlocks the file) and presses the row's "Finish" button,
     which the FE offers because the gate admits `partially_rolled_back` and promises it "re-attempts every item with a
     fresh recheck".
  3. The second run's page query keeps only `outcome = done` rows. Every item is now `rolled_back` or `skipped`, so the
     stream is empty.
  4. `resolve_final_state(0, 0, false)` ⇒ `RolledBack`. The op is marked fully rolled back and the gate refuses any
     further attempt (`AlreadyRolledBack`).
- **Trigger:** any partial rollback followed by "Finish rollback" from the history dialog (or a second
  `operations_rollback` from an agent).
- **Guard ruled out:** none re-reads skipped rows; the canceled-run path is fine only because un-attempted rows stay
  `done`. No planner test covers a second pass after skips (`planner_tests/` has only cancel-then-resume and
  reconcile-then-reissue cells).
- **Impact:** no file is destroyed, but the files that were left behind can never be undone from Cmdr again, and history
  tells the user everything came back. Recoverable by hand only if the user knows where to look.
- **Suggested fix:** stream `outcome IN (done, skipped)` on a rollback of a `partially_rolled_back` op (the per-item
  recheck keeps it idempotent), or resolve an empty pass over a `partially_rolled_back` op to stay partial.

### oplog-4. A journal read error mid-stream ends the rollback as "fully rolled back"

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operation-log/src/rollback.rs:432`, `execute_rollback`;
  `crates/cmdr-operation-log/src/rollback.rs:299`, `resolve_final_state`
- **Excerpt:**

```rust
432	    'pages: loop {
433	        let page = match read_rollback_units_page(&conn, &original.op_id, before, ROLLBACK_PAGE) {
434	            Ok(p) => p,
435	            Err(e) => {
436	                log::warn!(target: "operation_log", "rollback: page read failed: {e}");
437	                break;
438	            }
439	        };
```

- **Mechanism:** a page read error (`SQLITE_BUSY` past the 5 s timeout, a decode error on one row such as an unknown
  token written by a newer build that shares the schema version, an I/O error, or a dir row GC'd mid-stream) breaks out
  with `canceled = false`. The deferred-dir phase runs over whatever was buffered, then
  `resolve_final_state(reversed, skipped, false)` returns `RolledBack` when nothing was skipped. Every row after the
  failure is never visited.
- **Trigger:** rare: a transient DB error or one undecodable row during a rollback of a large op.
- **Guard ruled out:** the connection-open failure path (line 395) correctly resolves `Rollbackable`; the page-error
  path has no equivalent. A `map_item_row` decode error on ANY row of a page fails the whole page, so one bad row hides
  up to 512 good ones.
- **Impact:** the remaining items stay at their destinations while history says the op was fully undone, and the gate
  then refuses a retry. No file destroyed.
- **Suggested fix:** treat a page error like a stop: set a flag that forces `PartiallyRolledBack` (or `Rollbackable`
  when nothing was reversed) and finalize the inverse `Failed`.

### oplog-5. Rolling back a copy to a remote volume deletes a file the user has since edited, if its size didn't change

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operations/src/journal.rs:247`, `record_volume_transfer_source` (mtime always `None`);
  `crates/cmdr-operation-log/src/rollback.rs:598`, `remove_file_if_unchanged`;
  `crates/cmdr-operation-log/src/rollback.rs:224`, `verify_snapshot`
- **Excerpt:**

```rust
247	    if source_is_dir {
248	        for leaf in created_files {
249	            let rel = leaf.path.strip_prefix(dest_root).unwrap_or(&leaf.path);
250	            let source_leaf = source_root.join(rel);
251	            record_volume_leaf(
252	                scope,
253	                EntryType::File,
254	                source_volume_id,
255	                &source_leaf,
256	                Some((dest_volume_id, &leaf.path)),
257	                leaf.identity.recorded_size().and_then(|size| i64::try_from(size).ok()),
258	                None,
259	                overwrote,
260	                ItemOutcome::Done,
```

- **Mechanism:** every volume copy leaf (SMB, SFTP, S3, WebDAV, MTP, ADB, and Mac↔remote) records size and
  `mtime = None`. `verify_snapshot` then proves identity on size alone, so `remove_file_if_unchanged` deletes whatever
  same-size file sits at the path, permanently (`Volume::delete`, no trash).
- **Trigger:** copy files to a NAS or server; later edit one in place without changing its length (a fixed-size
  container or VM disk image, a SQLite database, a binary patched in place, a document re-saved over the same bytes
  count), or replace it with a different same-size version; then roll back the copy.
- **Guard ruled out:** this is a documented tradeoff (`cmdr-operation-log/DETAILS.md` § "What a volume leaf's snapshot
  carries"), but the stated reason only rules out the SOURCE's mtime captured at read time. Reading the destination's
  own mtime back right after landing would match a later unchanged stat on the same backend and catch every in-place
  edit. The local path doesn't have this gap (it records the preserved source mtime).
- **Impact:** permanent loss of the user's later edits to that file.
- **Suggested fix:** record the landed file's own `modified_at` (one `get_metadata` after the rename-into-place, or what
  the backend's write call returns) as the row's mtime; backends that can't report one keep `None`.

### oplog-6. Rolling back a copy whose source has since been deleted permanently removes the only remaining copy

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `crates/cmdr-operation-log/src/rollback.rs:598`, `remove_file_if_unchanged`; executor
  `crates/cmdr-operations/src/rollback.rs:169` (`delete_clearing_blockers`, a permanent `Volume::delete`)
- **Excerpt:**

```rust
598	async fn remove_file_if_unchanged(
599	    volumes: &dyn VolumeLookup,
600	    runner: &dyn RollbackRunner,
601	    unit: &RollbackUnit,
602	) -> ItemResult {
603	    let (vol_id, path) = removal_target(unit);
604	    let Some(volume) = volumes.get(&vol_id) else {
605	        return ItemResult::Skipped(SkipReason::Failed);
606	    };
607	    let live = match volume.get_metadata(&path).await {
```

- **Mechanism:** the inverse of a copy checks only the destination. It never asks whether the copy's source
  (`unit.source_path` on `unit.source_volume_id`, already in the row) still exists. A copy whose original has since been
  deleted, emptied from the trash, or wiped with a phone/card is no longer a duplicate, and its "undo" deletes it
  permanently (not to the trash).
- **Trigger:** copy photos off a phone or SD card, delete them there (a permanent delete, which isn't undoable and
  doesn't block the copy's undo), then later roll back the copy from history, or an Ask Cmdr agent calls
  `operations_rollback` on it. Also requires the source volume to be connected (the gate demands it), which is the usual
  case for the Mac's own disk.
- **Guard ruled out:** the confirm dialog says the undo deletes what the copy made, which is true but doesn't say the
  original is gone; the gate checks volumes are connected, not that the originals exist.
- **Impact:** permanent loss of the user's only copy of those files.
- **Suggested fix:** before removing a copy, stat its source; if absent (or drifted), skip with a new typed reason ("the
  original is gone"), or move the copy to the trash instead of deleting it.

## Near misses

- **Double dispatch race** (`rollback_operation` reads the state on a read connection, then sets `rolling_back`): two
  concurrent dispatches can both pass, but both reversals share lanes and every per-item act is an idempotent recheck,
  so the second pass finds `AlreadyGone`.
- **Retention pruning an op mid-rollback**: `prunable_ops_fragment` excludes `rolling_back` ops and their targets, and
  the unfinished inverse is excluded by `ended_at IS NOT NULL`.
- **Retention between the gate and `set_rolling_back`**: both go through the one writer thread; a prune in that gap
  would make the run read zero rows and report `RolledBack` (the oplog-4 shape), but only at the exact 6-hour tick for
  an op crossing the age cutoff. Too narrow to report alone.
- **Inode equality across devices on the `root` volume**: `is_self_collision` compares bare inode numbers, but a
  cross-device `std::fs::rename` fails `EXDEV`, so a coincidental match can't overwrite.
- **Cross-volume restore over an occupant appearing after the check**: `move_file_across_volumes` lands with
  `LandingName::ExpectedFree`, which refuses an occupied name.
- **Restore into a missing parent**: `create_directory_all` only adds.
- **Migrations**: forward-only, one transaction per step, downgrade refused untouched, unparseable file set aside before
  recreate; a corrupt `schema_version` text parses to 0 and makes `migrate_v1_initial` fail on `CREATE TABLE` (refused,
  not wiped).
- **Rollback of a rollback**: the inverse journals rows with no dest, so a redo's `restore_move` skips every item as
  `Failed`; harmless but a dead "Roll back" on a move/rename reversal row (low; `bookkeeping.rs:160`).
- **Create file undo**: `journal_instant_create` records no size or mtime, so `verify_snapshot` is `Unverifiable` and
  the undo always skips (fails safe; the feature just never works for created files).
- **Directory restore moves the whole subtree back, including files the user added to it since**: a move, not a loss.
- **Crash reconcile**: resolves `rolling_back` from the inverse's `done` rows; the inverse row itself is left `Running`
  with `ended_at NULL` forever (hygiene, history shows it running).
- **In-flight reversals (`reversal.rs`)**: local recheck uses `(dev, ino)` + size and `symlink_metadata`; volume recheck
  is size-only (same exposure as oplog-5, already documented in `transfer/DETAILS.md`).

## Covered

- `crates/cmdr-operation-log/src/rollback.rs`
- `crates/cmdr-operation-log/src/rollback/bookkeeping.rs`
- `crates/cmdr-operation-log/src/writer/prune.rs`
- `crates/cmdr-operation-log/src/retention.rs`
- `crates/cmdr-operation-log/src/store/migrations.rs`
- `crates/cmdr-operation-log/src/store/connection.rs`
- `crates/cmdr-operation-log/src/store/mod.rs` (lines 100–712)
- `crates/cmdr-operation-log/src/capture.rs`
- `crates/cmdr-operation-log/src/handle.rs`
- `crates/cmdr-operation-log/src/writer.rs` (lines 340–588; the message types above skimmed)
- `crates/cmdr-operations/src/rollback.rs`
- `crates/cmdr-operations/src/reversal.rs`
- `crates/cmdr-operations/src/journal.rs`
- `crates/cmdr-operations/src/ledger.rs`
- `crates/cmdr-operations/src/transfer/volume/move_file.rs`
- `apps/desktop/src-tauri/src/operation_log_host/mod.rs`
- Followed out of the slice (partial reads): `rename.rs` (journal snapshot + local/volume rename),
  `rename/bulk/recorder.rs`, `transfer/copy/single_item.rs` record points, `transfer/move_op/same_fs.rs`,
  `transfer/volume/move_same.rs`, `transfer/volume/copy_serial.rs`, `delete/trash.rs`, `unblocked_removal.rs`,
  `cmdr-local/src/local_posix.rs` (`rename`), `cmdr-local/src/reading.rs` (inode), `cmdr-fs/src/entry.rs` (`inode`), the
  FE's `operation-log-labels.ts` and `trash-undo.ts`.

## Not covered

- `crates/cmdr-operation-log/src/query.rs`, `types.rs`, `recording.rs`, `rollback/order.rs`, `rollback/skips.rs`,
  `rollback/runner.rs`, `store/mod.rs` lines 1–100, `writer.rs` lines 1–340 (message definitions and spawn).
- `crates/cmdr-operations/src/journal_search.rs`, `journal_capture*` (test files), and the cross-FS move's
  `JournalDestUnder` rebase.
- `apps/desktop/src-tauri/src/operation_log_host/volumes.rs` (test-only).
