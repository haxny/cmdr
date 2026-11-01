# Synchronize folders (app side) details

## sync-profiles.json

```json
{
  "_schemaVersion": 1,
  "entries": [ { "id": "…", "name": "Photos", "left": { "volumeId": "root", "path": "/a" }, "right": { … },
                 "options": { … }, "showFilters": { … }, "restoreShowFilters": false, "overrides": { … } } ],
  "lastOptions": [ { "left": { … }, "right": { … }, "options": { … } } ]
}
```

- **Schema rules.** Version 1 reads and writes this shape. A file with another version (a newer build's, say) is set
  aside by `state_recovery`, not read half; the list starts empty and the next save writes the current version, so a
  newer build's data survives only in the set-aside copy. A future migration hook belongs in `read`, between the
  version check and the envelope: accept versions below the current one and upgrade their entries before returning. A
  field added later is additive: an older build keeps it through a save because entries are raw JSON objects (see
  below), but drops `lastOptions` entries it can't read. Bump the version only for a change an older build must not
  touch.
- **Why raw values.** The typed `SyncProfile` is the contract of THIS build. Keeping the stored object and merging the
  typed fields into it (`merge`, recursively for objects, replacing everything else) keeps what a newer build added
  inside `options` or `overrides` too. The one trap is a serde-tagged value (`KeepPolicy::location`): merging a
  `custom` location over a stored `targetRoot` would leave both keys, which no build reads. `merged` therefore reads
  the result back and falls back to the typed value whole when it isn't exactly the saved profile.
- **Why not `settings.json`.** It is sparse and per-setting (a value exists only when changed, with migrations keyed by
  setting id). Profiles are ordered records with their own lifecycle, validation, and recovery.
- **Replace rules.** `save_in` finds the target entry by id, else by name (trimmed, lowercased). A match keeps its
  place; any other entry with the saved name is removed, so a list can't hold two profiles with one name.
- **Recovery.** `StateFile::SyncProfiles` names the file in the recovery notice (`stateRecovery.syncProfiles.title`). A
  killed write's `.json.tmp` leftover is removed at load.
- **Per-profile overrides** (`ProfileOverrides`) are `None` to follow Settings. The profile editor, with the inherited
  value greyed next to each override, arrives with the dialog task.
- **Roots.** `roots_conflict` (same volume only, component-wise, case-sensitive) is for the dialog to refuse nested or
  identical roots. Whether two spellings name one folder on a case-insensitive volume is the volume's business, not
  decided here.
- **Write order and failure.** `edit_at` holds `disk` across the change and its write (lock order `disk`, then
  `cached`), so two windows can't write out of order and the file keeps the newest change. A write that doesn't land
  leaves the change in memory and answers `WriteFailed`, which every command passes on typed (`ProfileSaveRefusal` or
  `ProfileEditRefusal`); the dialog says it isn't saved.
- **Rename vs save.** `rename_in` refuses a name another profile has (`NameTaken`); `save_in` replaces in place. The
  dialog asks, then saves over.
- **No id.** An entry without a string `id` is listed `Invalid`/`Unreadable` under the key `no-id-<place>`, which
  delete, rename, and move accept, so it can be removed.
- **Limits.** `effective` clamps frontend-supplied Settings values (tolerance 0 to 60 s, removal percentage 1 to 99,
  count up to 1,000,000); 100% would disable the guard, so Settings stops at 99.

## Sessions

One session per open dialog (`session.rs`, `SESSIONS`), addressed by a uuid the frontend holds. `start_sync_compare`
opens one when `sessionId` is `None`, else compares again in that one: the newer compare cancels the older's
`SyncControl`, and only the newest generation's result is stored (`Session::generation`). A compare checks every input
before it scans: each volume is connected (`VolumeNotConnected`), each root path is non-empty and never climbs with `..`
(`BadPath`), `roots_conflict` (`RootsConflict`), the masks parse (`BadMask`), the Settings with the profile's overrides on
top pass `validate_defaults` (`BadSettings`, besides the clamp in `effective`), the mode can write where it must
(`SideReadOnly`), and "Only selected" reads each pane's names through the listing engine, which must still show that
root at that sequence (`StaleSelection`). Both sides scan at once (`tokio::join!`); progress goes out at most every
100 ms; Content compares run after the plan and `reclassify` folds them in.

The session keeps both snapshots, the plan with every override applied (`set_sync_actions`: `bulk_changes` on the rows
the person selected AND can see, then `apply_overrides`, all or nothing), and the scan's options without the selection
(the engine re-checks folders with them). Grid pages are cut from `view_rows` per call. A run marks the session `stale`
(the folders changed); the dialog compares again.

Decision: the confirm (`get_sync_confirm`) and the run read the plan the session holds now. The run calls `gate` itself
at accept time, so a report shown before the person changed actions can't wave new removals through, then keeps only
the rows whose confirm group the person left ticked. Groups: new copies, replacements, removals per side, and copies
that turn the mode's removal around ("recreates": a right-only row reversed from `×→` to `←`).

**Run lock.** One run per pair of roots at a time, either order, across sessions (`RunLock`, released on drop, so a
panicking run task releases it too). A second start answers `RunInProgress`.

## Engine adapter

`engine.rs` implements `cmdr_sync::SyncEngine` over `cmdr_operations::Operations`; each batch is ordinary engine
operations with `Initiator::User`, so the queue window, progress, journal, and undo see them.

- **observe** reads `get_metadata` (lstat: a symlink is a `Symlink`, never followed) and, for a file or link, captures
  the engine's own `SourceFingerprint` (`capture_local` on a volume with a local path, else `capture_remote`, the rule in
  `source_binding.rs`). A folder's subtree comes from `subtree_now` with the compare's exact `ScanOptions`; a walk that
  stops or fails leaves the subtree `None`, which never matches a snapshot and never reads as empty. After a cancel the
  walks run on a fresh control: restore and tidy need a true answer.
- **Binding.** Each `BoundSource` is held to the fingerprint the last observe took (or one taken now), which must agree
  with the snapshot's facts (kind; size and time in whole seconds where both are known; nanoseconds are floored). A
  source that doesn't is `Skipped` and never reaches the engine; the rest go with `ExpectedSources`, so the engine
  re-checks the same identity right before acting.
- **Landing.** Copy, Move, and MoveBack use `ConflictResolution::Skip`; a move or trash is `Done` only when the engine
  says the source left its place. `ClaimFolder` makes the folder through `create_directory_managed` only if nothing is
  there (exclusive). `RemoveEmptyFolders` is one `std::fs::remove_dir` (or `Volume::delete_directory`, non-recursive by
  contract) after an lstat shows a real folder; hidden entries the scan skipped are left to the `rmdir` refusal, not
  counted. Trash is `trash_files_start`. ❌ Nothing calls a delete.
- **Cancel.** `run` forwards the run's cancel into the running operation (`cancel_operation`); a queued op is dropped
  without settling, so after a forwarded cancel the adapter also watches the queue and returns stopped once the op left
  it. `run_to_end` (move back, keep policy) never forwards it.
- **Why `SettleSink` wraps the `TauriEventSink`.** Replacing it would hide the op from the queue window and swallow the
  engine's questions (Locked items, sources gone). The wrapper forwards every event untouched, records each source's
  last outcome (a cross-volume move speaks twice), and wakes the adapter on `emit_settled`.

Paths: a volume with a local path gets `root_anchored` paths (the engine and the OS see the same file); others pass the
volume path as is. The custom versions folder's volume is the local disk (`LOCAL_DISK_VOLUME_ID`).
