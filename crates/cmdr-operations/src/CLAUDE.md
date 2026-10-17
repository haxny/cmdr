# Write operations

Copy, move, delete, trash, and zip edits as managed background ops.

## Module map

- Spine: `manager.rs` (registry, lanes, admission), `state.rs` (op state, cancel/abort), `ledger.rs` + `reversal.rs`
  (the in-flight ledgers), `status_cache.rs` (status + Eject's busy-volume set; reach it through `state::`), `types.rs`,
  `mod.rs`. Rest: DETAILS § "Files (top level)". Frontend: `apps/desktop/src/lib/file-operations/CLAUDE.md`.

## Must-knows

- **`spawn_managed` for copy/move/delete/trash, `run_instant` for rename/mkdir/mkfile**; blocking work runs in
  `spawn_blocking`, so a starter returns an id before I/O.
- **An instant op emits no `WriteCompleteEvent`** (analytics WRAPS its driver): ❌ never emit inside one, or the
  in-archive early return drops in-zip ops. It refuses with a typed `MutationError`, ❌ never a sentence.
- **A spawned op reserves every lane it touches or waits Queued**; the next admits on `on_settled`, ❌ never `Drop`.
- **The intent's writers are `IntentCell`'s** (`operation_intent.rs`). Cancel keeps copied files; Rollback removes what
  it still recognizes, VERIFYING before each destructive act (❌ never a batch or a fork; only the `Drop` net is
  unconditional). Pause is orthogonal, cancel wins. A wind-down reversal sees its op only as a `WindDown`.
- **A loop parks where it checks cancel**, via `state.stop_or_park_sync()` / `_async()`, ❌ never hand-rolled; a SCAN's
  own is `ScanPause`, owing `note_parked`. **Parking on a PERSON owes both edges**, the `human_wait.rs` clock AND
  `announce_human_wait(sink)`, or the ETA lies. Clicks stop via `backend_cancel`; `backend_abort` /
  `cancel_all_write_operations` are the quit deadline's.
- **`write-conflict` takes an `ArmedConflict`** (`conflict_slot.rs`), so the slot is armed first; ❌ the dispatch mutex
  never spans a write. An answer NAMES its clash (`ConflictId`): ❌ never fuse `AlreadyResolved`, `StaleAnswer`, or
  `NoPendingConflict`, nor leave a settled prompt up.
- **Crossing types takes a person's consent, per SHAPE** (`conflict::resolution_for_clash`, ALL THREE engines): only a
  plain Overwrite answered for that `ClashKind` replaces, ITS "* all" carry included; the config policy, same-kind
  carries, and `OverwriteSmaller`/`Older` `Skip`. ❌ Never reduce a conditional before it. What ARRIVES is the caller's
  word (`IncomingItem`).
- **Emit through `OperationEventSink`, ❌ never `AppHandle`** (spawns: `host/`); `write-settled` fires once, AFTER the
  terminal event. A cross-FS move speaks TWICE per source on `write-source-item-done`, so the LAST wins.
- **EVERY local write lands via `overwrite::stage_and_land_file`** (temp+rename, rename-aside when replacing); announce
  each destination BEFORE the syscall: the writers take it as `host::Announced` (a rename's SOURCE too, by hand).
  Leftovers carry a KIND (❌ only a `temp` goes on sight); ❌ only the installed engine's `claim_ledger` opens the
  ledger. DETAILS § "What the sweep does with each kind".
- **Every mutation journals through its `OpScope`**; a VOLUME op passes its REAL volume id. Bulk rename journals each
  hop as it lands: ❌ never batch to the end, nor put a rotation temp in `in_flight_temps` (its sweep DELETES it).
- **A transfer carries both volumes as typed sides** (`transfer_sides.rs`), captured at start: a vanished drive can't be
  named later. A drive that LEFT is the mount table's answer, ❌ never an errno.
- **Free space: `free_space.rs`**, `statvfs` first, slow NSURL only on a shortfall; a shortfall is the person's call
  (`SpaceShortfall`). Copy reserves `total_bytes`, ❌ never the dedup'd `dedup_bytes` (delete's).
- **❌ The `types` vocabulary floor `use`s no sibling**, `types/events.rs` included. It holds `LifecycleStatus`, the ONE
  lifecycle answer: ❌ never re-derive it from a presence test, no new variant.
- **Every preview runs under a `ScanWatchdog`**, bounded by INACTIVITY: feed the progress callback, and whoever settles
  it CLAIMS the outcome.
- **A FAILED op is retained out-of-band**, the one exception to removal; `retain_failure` emits only after the record is
  GONE. **A test owns its engine** (`testing::TestOperations`), ❌ never a `static`.

Architecture, flows, and decisions: `DETAILS.md`; read it before any non-trivial work here.
