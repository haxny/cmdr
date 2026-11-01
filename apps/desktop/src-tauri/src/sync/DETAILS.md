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
- **Store shape.** The store follows `multi_rename/preset_store.rs`: temp+rename writes, two locks never held at once.
  An invalid profile is listed greyed with its reason, and `save_in` refuses one that fails `validate`; only an entry
  with no string `id` is left out of the list (it stays in the file). The list holds `MAX_PROFILES` (200); the newest
  `MAX_REMEMBERED_PAIRS` (50) ordered pairs of roots keep their last options. The backend's say on a versions folder in
  Settings is `check_sync_keep_policy`.
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
opens one (owned by the calling window; `WindowEvent::Destroyed` closes its sessions) when `sessionId` is `None`, else
compares again in that one: the newer compare cancels the older's `SyncControl`, only the newest generation's result is
stored (`Session::generation`), and a superseded compare emits no progress and no `sync-compare-finished`. Both events
carry the generation, which `start_sync_compare` answers too. A compare checks every input
before it scans: each volume is connected (`VolumeNotConnected`), each root path is non-empty and never climbs with `..`
(`BadPath`), each local-namespace root resolves to its real path (`canonicalize`, 5 s at most, else
`RootUnreachable`), the roots don't overlap (`RootsConflict`: by real path when both have one, whatever volume ids named
them, so a symlink to the other root or one folder under two ids is caught; else `roots_conflict` on one volume), the masks parse (`BadMask`), the Settings with the profile's overrides on
top pass `validate_defaults` (`BadSettings`, besides the clamp in `effective`), the mode can write where it must
(`SideReadOnly`), and "Only selected" reads each pane's names through the listing engine, which must still show that
root at that sequence (`StaleSelection`). Both sides scan at once (`tokio::join!`); progress goes out at most every
100 ms; Content compares run after the plan and `reclassify` folds them in.

The session keeps both snapshots, the plan with every override applied (`set_sync_actions`: `bulk_outcome` on the rows
the person selected AND can see, then `apply_overrides`, all or nothing), and the scan's options without the selection
(the engine re-checks folders with them). `set_sync_actions` answers `SyncActionsOutcome`: the new state, the rows it
left alone grouped by `RowSkipReason`, and how many selected rows the filters hide, so the dialog is never silent; a
key that changes nothing doesn't bump the revision. Every `SyncState` carries its plan revision (revisions only grow),
so the dialog drops a late summary. Grid pages are cut from `view_rows` per call; each item row carries both sides'
paths in their own spelling (`leftRelPath`, `rightRelPath`) for F3/F4. `find_sync_row` answers the dialog's jumps
(next or previous folder, a typed name) from the same cached view (`cmdr_sync::find_row`). A run marks the session `stale`
(the folders changed); the dialog compares again.

Decision: the run is tied to the confirm the person saw. `Compared::revision` (unique across sessions) changes with
every override, content answer, and confirm shown with other Settings; `ConfirmPreview::revision` names it, and
`start_sync_run` refuses `PlanChanged` unless `RunRequest` names the current revision and the same Settings. A compare a
run has already changed is `StaleCompare`. The volume a compare read must still be the instance under its id
(`Arc::ptr_eq`), else `VolumeNotConnected` (compare again). Both local roots are resolved again (`real_paths.rs`, off
the session lock): a root that now resolves elsewhere (swapped for a symlink since the compare) is `StaleCompare`, one
that won't resolve is `RootUnreachable`. Below the root, `no_link_below` covers each write.

Real paths are resolved on a blocking thread with a 5 s limit and never under a session lock: the roots at compare and
run, a custom versions folder at confirm and run. For a custom folder only "not there" climbs to the parent (it may be
made later); any other error is `KeepFolderUnreadable`, since a folder that won't answer can't be shown to lie outside
the roots.

Gap: a protocol volume (Cmdr's own SMB client, SFTP, WebDAV…) has no local path, so its roots compare by volume id and
path only. The same share mounted in Finder (a local path under `/Volumes`) is not recognised as the same folder: the
same-or-nested check and the run lock can't match the two.

The confirm (`get_sync_confirm`) and the run read the plan the session holds now. The run calls `gate` itself
at accept time, so a report shown before the person changed actions can't wave new removals through, then keeps only
the rows whose confirm group the person left ticked. Groups: new copies, replacements, removals per side, and copies
that turn the mode's removal around ("recreates": a right-only row reversed from `×→` to `←`).

**Run lock** (`run_lock.rs`). A run holds both its roots across sessions; a new run is refused (`RunInProgress`) when
either of its roots is, holds, or lies inside a held root. Local-namespace roots compare by real path in one shared
namespace, others by volume id and path; paths compare by components, so a trailing `/` changes nothing. Released on
drop, so a panicking run task releases it too.

**Content compare of one row** answers only onto its own row: refused while a run goes on (`RunInProgress`), and an
answer for an older compare generation or a row whose paths changed under that index is `PlanChanged`. Grid pages are
cut from a `view_rows` result cached per plan revision and show-filters.

## Engine adapter

`engine.rs` implements `cmdr_sync::SyncEngine` over `cmdr_operations::Operations`; each batch is ordinary engine
operations with `Initiator::User`, so the queue window, progress, journal, and undo see them.

- **observe** reads `get_metadata` (lstat: a symlink is a `Symlink`, never followed); only `NotFound` is "nothing
  there", any other error reads as a folder of unknown contents (never matches a snapshot, never empty, never absent).
  For a file or link it captures
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
- **No symlink leads a write.** Right before a copy, move, or move back lands, and before folders are made or an
  empty one removed, every existing folder from just below the side's root down to the destination is lstat'ed; a
  symlink there, or a folder that won't answer (anything but `NotFound`), refuses the batch (`Failed`). The local disk's custom folder is checked by the run from the folder the
  person picked (`safe_way`). Roots themselves may be symlinks (resolved once, at compare).
- **Real paths and `RunSide`.** `RunSide::root` stays the volume path (the volume resolves paths under its own root,
  which may itself be a symlinked path); `RunSide::namespace_root` is the real path, so `InsideSyncFolder` sees through
  symlinks, and the app also resolves a custom folder's existing prefix before comparing it with the real roots.
- Warn logs name a refusal's typed kind (its serde tag), never its paths.
- **Cancel.** `run` forwards the run's cancel into the running operation (`cancel_operation`); a queued op is dropped
  without settling, so after a forwarded cancel the adapter also watches the queue and returns stopped once the op left
  it. `run_to_end` (move back, keep policy) never forwards it.
- **Why `SettleSink` wraps the `TauriEventSink`.** Replacing it would hide the op from the queue window and swallow the
  engine's questions (Locked items, sources gone). The wrapper forwards every event untouched, records each source's
  last outcome (a cross-volume move speaks twice), and wakes the adapter on `emit_settled`.

Paths: a volume with a local path gets `root_anchored` paths (the engine and the OS see the same file); others pass the
volume path as is. The custom versions folder's volume is the local disk (`LOCAL_DISK_VOLUME_ID`).
