# Data-safety hunt 2026-10: operation log verification (`oplog`)

Verifier pass over `oplog-hunt.md`. Every excerpt was checked against the current tree (worktree `data-safety-hunt-2`,
HEAD `22918588b`) and is verbatim. All six survive; two get a different severity.

### oplog-1

- **Verdict:** confirmed
- **Severity:** high (agree)
- **Evidence:**
  - `crates/cmdr-operation-log/src/rollback.rs:284-289`: `is_self_collision` falls back to `fold_path` equality whenever
    either inode is `None`. `fold_path` (`rollback.rs:263`) uses `fold_name` (`store/mod.rs:48`,
    `to_lowercase().nfc()`), with no per-volume case-sensitivity input anywhere: `rg -i case_sensitiv` over
    `cmdr-operation-log/src` and `cmdr-fs/src` finds nothing relevant.
  - `crates/cmdr-local/src/reading.rs:316`: `inode` is `Some` only for `is_file() && nlink() > 1`, so
    `LocalPosixVolume::get_metadata` (`local_posix.rs:378` → `get_single_entry`) returns `None` for an ordinary file.
    The doc comment at `rollback.rs:275` ("Where real inodes exist (`LocalPosixVolume`)") is wrong about that. No remote
    backend sets `inode` either.
  - `rollback.rs:727-735`: fold-equal ⇒ `force = true`, the occupant check is bypassed.
  - Executor `crates/cmdr-operations/src/rollback.rs:203`: `from.rename(from_path, to_path, force)`.
    `LocalPosixVolume::rename` with `force` is `std::fs::rename` (`local_posix.rs:620-625`), which replaces the target;
    SFTP's `rename_over` (`cmdr-sftp/src/volume/mutation.rs:221-240`) uses `posix-rename` or probes and removes the
    target, then renames.
  - `rollback/decision_tests.rs:81-90` pins "no inodes, same volume, case-only ⇒ self", with no case-sensitive-occupant
    cell.
- **Refutation attempts:** the snapshot check at `rollback.rs:712` only verifies the `from` side; nothing re-lists the
  parent to see whether two distinct entries exist; the `AlreadyExists` mapping (`rollback.rs:757`) never fires because
  `force` skips the exclusive path. On the default case-insensitive APFS the fold is correct (one entry), so the bug
  needs a case-sensitive volume, which SFTP/S3/case-sensitive APFS/Linux all are. Real, distinct content is destroyed in
  place, not trashed.

### oplog-2

- **Verdict:** confirmed
- **Severity:** medium (hunter: high)
- **Evidence:**
  - `crates/cmdr-operation-log/src/store/migrations.rs:159-160`: `dirs_identity` is
    `UNIQUE (volume_id, IFNULL(parent_dir_id, 0), name_folded)`.
  - `store/mod.rs:334-358`: `intern_one` does `ON CONFLICT … DO NOTHING` and reads the id back by `name_folded`, so
    `/srv/media` resolves to an existing `/srv/Media` row and keeps its stored `name` (first spelling wins).
  - `store/mod.rs:364-388`: `reconstruct_dir_path` rebuilds from the stored `name`, so the rollback unit's path points
    at `/srv/Media/...`. Leaf names (`source_name`, `dest_name`) are stored exact; only the folder chain folds.
  - `rollback.rs:598-627`: `remove_file_if_unchanged` stats that wrong path: `NotFound` ⇒ `missing_item` ⇒ counted
    reversed (false "rolled back", the real copy stays); present and size(+mtime) match ⇒ `RemoveFile` on a file in a
    folder the op never wrote.
- **Why medium:** the false "rolled back" branch is a wrong result with nothing destroyed. The delete branch needs a
  same-named file of the same size (plus the same mtime on a local leaf) in the case-twin folder; when it's a true
  duplicate of what was copied, the content survives in the real destination, so permanent loss of distinct content
  needs a same-size-different-content coincidence (realistic only on size-only remote leaves). A restore-move landing in
  the case-twin folder is misplacement, and an occupant there is skipped. That's "wrong result, recoverable" for the
  common shape, with a narrow loss tail; still worth fixing together with oplog-1, since both come from the same
  assumption that folding is always safe.

### oplog-3

- **Verdict:** confirmed
- **Severity:** medium (agree)
- **Evidence:**
  - `store/mod.rs:625-635`: `read_rollback_units_page` filters `outcome = done` only.
  - `rollback/bookkeeping.rs:49-56`: a non-reversed skip writes `ItemOutcome::Skipped` onto the ORIGINAL item;
    `rg ItemOutcome::Skipped` finds no code that ever resets it to `done`.
  - `rollback.rs:321-333`: `check_rollbackable` admits `PartiallyRolledBack`; FE `operation-log-labels.ts:120` promises
    "re-attempts every item with a fresh recheck".
  - Second pass streams zero rows ⇒ `resolve_final_state(0, 0, false)` ⇒ `RolledBack` (`rollback.rs:308-309`), and the
    gate then returns `AlreadyRolledBack` (`rollback.rs:324`).
- **Refutation attempts:** the doc comment at `rollback.rs:297-298` says skips "won't clear on a retry", which might
  read as intent, but then the empty retry resolving to `RolledBack` contradicts the same function's "any skip ⇒
  partial" rule, and the FE copy promises a re-attempt. No planner test runs a second pass after skips (`planner_tests/`
  covers cancel-then-resume and reconcile only). No file destroyed; history becomes false and the leftovers become
  un-undoable.

### oplog-4

- **Verdict:** confirmed
- **Severity:** medium (agree; rare trigger)
- **Evidence:** `rollback.rs:432-439`: a page-read `Err` logs and `break`s with `canceled` still `false`; the
  deferred-dir phase runs (`rollback.rs:500-510`), the inverse is finalized `Done`, and
  `resolve_final_state(reversed, skipped, false)` yields `RolledBack` when nothing was skipped. Contrast the
  open-failure path at `rollback.rs:395-415`, which correctly resolves `Rollbackable` and finalizes `Failed`.
  `read_rollback_units_page` (`store/mod.rs:637-644`) fails the whole page on any one `map_item_row` /
  `map_rollback_unit` error. No file destroyed.

### oplog-5

- **Verdict:** confirmed
- **Severity:** medium (agree)
- **Evidence:** `crates/cmdr-operations/src/journal.rs:247-271`: both branches of `record_volume_transfer_source` pass
  `mtime = None`. `verify_snapshot` (`rollback.rs:224-258`) then matches on size alone, and `remove_file_if_unchanged`
  deletes via `delete_clearing_blockers` → `Volume::delete` (`cmdr-operations/src/rollback.rs:169-170`,
  `unblocked_removal.rs:54`), permanent. `cmdr-operation-log/DETAILS.md:729-733` documents no-mtime as deliberate, but
  its reason (the SOURCE's mtime isn't preserved, so it would always drift) doesn't rule out the hunter's fix of
  recording the DEST's own mtime read back after landing. Medium because the user asked to remove that copy; what's lost
  is later same-length edits to it.

### oplog-6

- **Verdict:** confirmed
- **Severity:** medium (agree)
- **Evidence:**
  - Inverse of a copy: `rollback.rs:182-186` maps a copy file to `RemoveFileIfUnchanged`; `remove_file_if_unchanged`
    (`rollback.rs:598-627`) touches only `removal_target(unit)` (the dest) and never reads `unit.source_path` /
    `unit.source_volume_id`.
  - Gate: `check_rollbackable` (`rollback.rs:321-346`) checks only the stored state and that both volumes are
    registered. No other upstream check exists: no "a later op touched this" / supersession logic in
    `cmdr-operation-log/src` (`rg -i "supersed|later op|blocked_by"` finds nothing).
  - Executor: permanent `Volume::delete` (`cmdr-operations/src/rollback.rs:169-170`), no trash.
  - Presentation: the history row's button is "Roll back" (described to translators as the button that "undoes that
    operation"), and the confirm is `RollbackConfirmDialog.svelte:61-65` variant `undoByDeleting`, body
    `fileOperations.rollbackConfirm.bodyUndoByDeleting`: "This deletes the files and folders the operation created. Cmdr
    skips anything it isn't sure about, so a few may stay behind." It's marked destructive (red) and honest about
    deleting, but it implies the copies are redundant and does no source check; "skips anything it isn't sure about"
    arguably promises the opposite of what happens here. The FE does no source check either.
  - MCP: `operations_rollback` needs only `autoConfirm: true`
    (`apps/desktop/src-tauri/src/mcp/executor/operation_log.rs:109-113`) and calls the backend directly, so an agent
    gets no dialog at all.
- **Why medium, not high:** the user explicitly confirms a dialog that says it deletes those files; the loss needs the
  original gone first. It's a fair gap since "undo a copy" carries the mental model "the copy is redundant", and the
  delete is permanent. Moving the copy to the trash, or skipping with a typed "the original is gone" reason, closes it.
