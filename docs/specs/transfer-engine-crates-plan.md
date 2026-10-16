# The transfer engine becomes four Tauri-free crates: execution plan

Just-in-time plan for GitHub issue #415. It moves the code that writes and deletes user files out of the app crate:

1. `LocalPosixVolume` and the local `std::fs` reader (`apps/desktop/src-tauri/src/file_system/volume/backends/`) →
   `crates/cmdr-local`.
2. The operation log (`apps/desktop/src-tauri/src/operation_log/`) → `crates/cmdr-operation-log`, handle `OperationLog`.
3. The write engine (`apps/desktop/src-tauri/src/file_system/write_operations/`) → `crates/cmdr-operations`, handle
   `Operations`.
4. Supporting cut: the name collator (`crates/cmdr-listing/src/collation.rs`) → `crates/cmdr-collation`, so neither
   `cmdr-local` nor `cmdr-operations` depends on the listing engine for one type.

David wants all of it as ONE effort: a series of green commits on one worktree branch. The parts are stepping stones
inside it, not separate deliveries.

The playbook this plan assumes is `docs/guides/multi-agent-refactors.md`. Before executing any milestone, read the
`CLAUDE.md` + `DETAILS.md` of every module the milestone touches, plus: `crates/cmdr-listing/CLAUDE.md` + `DETAILS.md`
(the shape to copy), `crates/cmdr-fs/src/volume/host/CLAUDE.md` (the existing seams, the runtime handle, the dispatch
rule), `crates/cmdr-archive/DETAILS.md` § "The pattern, for the next backend crate",
`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Architecture" (why moving a backend is never only a file
move), `apps/desktop/src-tauri/src/mtp/DETAILS.md` § the registry cycle ("the wiring knows the backend, the backend
never knows the registry"), and `scripts/check/checks/DETAILS.md` § "Rust module cycles".

The model is the listing extraction (#301, plan in git history at `748b4adfd^:docs/specs/listing-crate-plan.md`). Its
milestone shape (characterize → vocabulary down → seams → handle → rules-as-types → public API → tests own the engine →
move → docs) worked with zero regressions, so this plan reuses it, adding four engine-specific things: the operation log
as its own crate first, an event-wire spike run alongside the baseline, a frozen must-know list as the conformance
referee, and stricter install and ledger rules for an engine that writes.

## Precondition (met): the intent race is fixed on `main`

`cancel_write_operation` and `cancel_all` each used to load the intent, validate, then `store` it, with nothing spanning
the pair. A user's Rollback that loaded `Running`, then a quit's `cancel_all` storing `Stopped`, then the Rollback
storing `RollingBack`, turned a teardown ("must never silently delete files") into a rollback that deletes them (review
round 1, safety H1).

Landed on `main` in `a5b218a63`, as a standalone red-then-green fix outside this plan's milestones: `transition_intent`
(`operation_intent.rs:68`, `state.rs:438`) is atomic and table-checked, `cancel_all` calls
`state.transition_intent(Stopped)` (`state/registry.rs:105`), no production `intent.store` remains, and
`write_operations/CLAUDE.md` says the intent is written ONLY through it. The effort's worktree starts from a `main` at
or after that commit. M6d's `IntentCell` wraps `transition_intent`.

## Loud rules (read before touching anything)

- **Data safety first. This code writes, overwrites, moves, and deletes user files.** Every must-know in
  `write_operations/CLAUDE.md`, `transfer/CLAUDE.md`, `transfer/volume/CLAUDE.md`, `delete/CLAUDE.md`,
  `archive_edit/CLAUDE.md`, `transfer_tree/CLAUDE.md`, `transfer/transfer_driver/CLAUDE.md`, `operation_log/CLAUDE.md`,
  `volume/backends/CLAUDE.md`, `file_system/volume/CLAUDE.md`, and `file_system/CLAUDE.md` survives. M0a freezes them as
  a quoted list (§ "Frozen must-knows"); the conformance review checks against that list, not a summary. Where this plan
  turns a rule into a type, the rule's doc line shrinks to a pointer at the type; it never silently disappears.
- **Byte-identical behavior, except the changes in § "Behavior changes, called out".** Each lands as its own commit with
  a red test first, named in the commit body. Anything else that changes behavior is a bug in the milestone, not a
  judgment call.
- ❌ **Never loosen a test to make a move compile.** No weakened assertion, no `#[ignore]`, no widened timeout, no
  deleted cell. A test that can't follow the code stays app-side as an integration cell, unchanged, and the milestone
  report names it. If a test turns out to be wrong, that's a separate commit with its own reason.
- **Tests move with the code.** A test that needs only the crate (and `cmdr-fs` / `cmdr-local` / `cmdr-archive` fakes)
  moves into the crate. A test that needs the real app (the volume registry wiring, the Tauri runtime, virtual MTP, the
  Docker fixtures, the downloads watcher, real disk images) stays app-side. § "Tests: which move, which stay".
- **The TS bindings must not move.** Every event name, payload shape, command signature, and type name the frontend sees
  stays the same. `desktop-bindings-fresh` regenerates `apps/desktop/src/lib/ipc/bindings.ts` and must report zero diff
  on every milestone.
- **`main` ships after every milestone.** Each milestone compiles, passes its lanes, and leaves the app working.
  Temporary shims exist for exactly this; each one is marked `// shim: removed in <milestone>` and dies there. When a
  milestone runs long, stop at a green commit boundary and hand the rest on. Never leave the branch red.
- **No `tokio::spawn`, `tokio::task::spawn`, `tokio::task::spawn_blocking`, bare `std::thread::spawn`, or
  `tauri::async_runtime::*` in `cmdr-operations` and `cmdr-operation-log`.** Everything there spawns on the runtime of
  the `VolumeHost` the host passes in. A dedicated, NAMED `std::thread::Builder` thread is fine (the in-flight log
  writer, the operation-log writer, the launch sweep, the native-copy workers): those are the threads the `cmdr-index`
  QoS rule is about. Known exceptions, each documented where it lives: `cmdr_fs::deadline`'s ambient `spawn_blocking`
  (callers are already inside the runtime), the `mutation_reply` continuation if M4 decides it stays ambient (§ M4), and
  `cmdr-local`'s `Volume`-method `spawn_blocking` calls (`local_posix.rs:36`, ~15 sites), which every backend crate
  makes the same way (`cmdr-archive/src/volume.rs:41`) for the same reason. M1b and M1c leave those alone.
- **Until M8a, the engine's test scaffolding is plain `cfg(test)`.** The app crate has no `testing` feature, by decision
  (`apps/desktop/src-tauri/Cargo.toml:38-39`), and `feature = "testing"` there is an `unexpected_cfgs` error. The
  `backend_suites/` cells are child modules and see `cfg(test)` items. Each new crate gets a `testing` feature when it's
  created (M1a, M1c, M2b, M8a), widens exactly what app-side cells reach to `any(test, feature = "testing")`, and the
  app's `[dev-dependencies]` gains that crate with `features = ["testing"]` (the `Cargo.toml:459-477` pattern).
- **The host is a value, not a static.** No new `static` in any of the four crates, and every existing engine static
  becomes handle state (M2a, M6a, M6b). The documented exceptions are listed in § "Statics" with the reason each stays.
- **A seam is called per operation, per directory, or per file landing, never per chunk or per directory entry of a
  listing.** The dispatch rule from `crates/cmdr-fs/src/volume/host/CLAUDE.md`, sharpened for a write engine: a file
  landing already costs syscalls, so one `dyn` call beside it is noise, but a call inside a chunk loop is not. The
  Criterion benches from M0a are the referee.
- **No seam, observer, or sink is called while an engine lock is held.** M6c audits it.
- **A write engine never runs detached in production.** Before `install`, the app's accessor has no engine to hand out,
  and the IPC layer refuses with an existing error variant plus a `log_error!` (§ M6a); it never writes. Only the test
  binary gets a working lazily built engine. One in-flight ledger per data dir, claimed once, by the installed engine
  only. Cross-process exclusivity is already `instance_lock.rs`'s job (one Cmdr per data dir, taken at `lib.rs:288`
  before the sweep), so the ledger takes no lock of its own. A second process is refused whenever the lock can be taken,
  as today: on an I/O error the lock logs a warning and carries on unlocked (`instance_lock.rs:175-177`, the
  `AcquireError::Io` arm). That's existing behavior, which this effort neither adds nor fixes.
- **The engine reports facts; the app decides what the user sees.** Event payload envelopes with
  `#[tauri_specta(event_name)]` derives, analytics properties, error-report routing, and every English word stay
  app-side. `index-crate-isolation` forbids `tauri` and `tauri-specta` in the crates anyway.
- **Inside a crate, gate test scaffolding on `any(test, feature = "testing")`, never `cfg(test)` alone**, for anything
  an app-side test reaches. There are 371 `cfg(test)` sites in the two engine modules today; most stay `cfg(test)`, but
  every one an app-side cell uses widens at the move that puts it in a crate (the previous rule says when).
- **Lanes: only what the change touches; slow lanes sparingly.** Each milestone names the lanes its change actually
  reaches. `--include-slow` runs ONCE, at M9. A slow lane appears mid-effort only where the milestone genuinely needs
  it, with the reason beside it: `desktop-rust-module-cycles` at M2a, M5a, M6a, and M8a (each adds the first edge of a
  new kind between modules: an app accessor the engine calls, the seam slot, the handle and `OpScope`, the crate homes;
  the allowlist is down to one home, and a new tangle needs David's OK), `desktop-rust-clippy-linux` at M1b (Linux-only
  space and mount code moves), the disk-image lane and `desktop-svelte-e2e-linux` at M8b (the lane's atoms change; the
  user flows run end to end once after the move), and `desktop-rust-cargo-udeps` at M8b (dependency pruning is the
  change). Every milestone from M1b through M8a also runs the fast
  `cargo check -p cmdr --bench transfer_benchmarks --features cmdr/transfer-bench,cmdr/virtual-mtp`, because no lane
  compiles the bench (`required-features`).
- **Never a bare `cd`** in an executor (it moves every agent in the session). Use absolute paths and `git -C`, and run
  everything in the foreground.
- **Allowlists:** `file-length` / `jscpd-rust` entries re-path when files move (your call, with a `reason`).
  `lock-poison` entries for engine files must reach ZERO in M6c, never re-path. A new `module-cycles` home needs David's
  OK. The `index-crate-isolation` ceilings, initial and the two planned re-measures, are approved with this plan (§
  "Decided", item 7).
- **Strip milestone tags before merging** (`rg -n '\b(M[0-9][a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'` over touched
  paths). The shim markers above are the only sanctioned ones, and they die with their shims.

## Decided

David approved the effort; the lead decided the rest. Approving this plan approves the isolation ceilings (item 7).

1. **Four crates, layered bottom-up.** `cmdr-collation` (ICU only) and `cmdr-local` (`cmdr-fs`; `cmdr-collation` only if
   M1b finds the reader's sort must stay AND the `sort_listing` seam doesn't fit, which is unlikely) at the bottom;
   `cmdr-operation-log` (`cmdr-fs`; `cmdr-local` as a dev-dependency only); `cmdr-operations` on top (`cmdr-fs`,
   `cmdr-local`, `cmdr-collation`, `cmdr-operation-log`, `cmdr-archive`; `cmdr-listing` as a dev-dependency only, for
   `TestListing` in `create/tests.rs`). `cmdr-listing` re-exports `NameCollator` from `cmdr-collation`, so its callers
   don't move. `cmdr-operations` does NOT depend on `cmdr-index`: the index questions go through a seam, and
   `journal_search.rs`'s direct `IndexStore` reads move into the app's adapter. Rejected: folding `LocalPosixVolume`
   into `cmdr-fs` (shared vocabulary; the local backend brings objc2/NSURL and the space memo), and a normal
   `cmdr-operations → cmdr-listing` edge (every listing edit would recompile 160k lines of engine, and the build's long
   pole would become `cmdr-fs → cmdr-listing → cmdr-operations → cmdr`).
2. **Names: `cmdr-operations` / `Operations`, accessor `operations_host::operations()`; `cmdr-operation-log` /
   `OperationLog`, accessor `operation_log_host::operation_log()`; `cmdr-local` (no handle; a backend crate's API is the
   `Volume` trait); `cmdr-collation`.** "Operations" is what the UI says (operation queue, file operations,
   `operations-changed`, `src/lib/file-operations/`), it names everything the crate holds (copy, move, delete, trash,
   rename, create, archive edits), and it avoids `cmdr_transfer::transfer::`. The engine-wide seam is `EngineEvents`, so
   it never reads as the per-op `OperationEventSink`.
3. **Each engine is a handle the app builds once with explicit seams**, held app-side in a `OnceLock` behind its
   accessor, like `listing_host::listings()`. Tests build their own. Keeping the 21 production statics as crate statics
   would be a smaller diff and the wrong end state: it keeps every per-test guard (`TestOperationGuard`,
   `TestJournalGuard`'s process-wide lock, `private_manager()` / `leaked_manager()`, the `one_test_per_process()` gate).
4. **Install order and identity are explicit.** `operation_log_host::install()` runs first and RETURNS the handle;
   `operations_host::install(operation_log, …)` takes it by value (no second `operation_log()` read that could pick up a
   pre-install placeholder), then runs before today's in-flight sweep (`lib.rs:363-365`). `install` runs
   UNCONDITIONALLY: the data dir is an `Option` passed in, never the gate on whether `install` runs (today the sweep
   sits inside `if let Ok(data_dir) = config::resolved_app_data_dir(..)`, `lib.rs:349`; putting `install` in that `if`
   would leave a data-dir failure with an app that can't copy at all). Only the `OnceLock::set` winner calls
   `Operations::claim_ledger(data_dir)`, a consuming once-per-handle claim that replaces the public
   `init_and_sweep_in_flight_temps`; a losing duplicate is dropped untouched. The journal opens late:
   `OperationLog::open(db_path)` runs at today's `lib.rs:813` on the SAME handle, publishes writer and journal in ONE
   slot write AFTER reconcile, and a second `open` is `Err(AlreadyOpen)`. A test pins `Arc::ptr_eq` between the engine's
   log and `operation_log()`.
5. **The `mcp::terminal_ops` reverse dependency leaves through the observer seam that already exists:
   `OperationEventSink`.** The app's Tauri sink adapter (which moves app-side anyway, holding an `AppHandle`) keeps
   calling `terminal_ops::record`, the completion analytics, the `warn!` line, and `expect_space_change`, and calls a
   new public `Operations::retain_failure(…)` where today's sink calls `manager().record_failure(…)`. Byte-identical.
   Rejected: an engine-level "operation ended" observer. It would double-fire under `archive_edit/move_out.rs`'s
   `SuppressTerminalsSink` (which swallows the inner extract's terminals) and make every collector-sink test retain
   failures.
6. **Engine-wide facts get ONE seam, `EngineEvents`**: operations changed, busy volumes changed, transfer
   started/finished on volumes (indexing priority), staging leftovers kept, an instant mutation settled (analytics), and
   typed error reports (today's 12 `log_error!` sites). `cmdr-index` routes the same kinds of fact through its one
   `EventSink`. The instant-op analytics uses this rather than `VolumeHost::analytics()`: that seam's `&[(&str, &str)]`
   shape is for backend counters, and the count bucket and property types are app policy (`analytics.rs` builds `json!`
   values today).
7. **Initial `index-crate-isolation` ceilings, measured with no headroom** at M1a (`cmdr-collation`), M1c
   (`cmdr-local`), M2b (`cmdr-operation-log`, HandleType `OperationLog`), and M8a (`cmdr-operations`, HandleType
   `Operations`), plus two planned re-measures, also without headroom: `cmdr-operation-log` at M6d (`OperationId` on the
   journal API) and any crate M4 touches. All approved with this plan; any other rise needs David's OK. (`cmdr-local` is
   complete when first measured: the mount probes move in M1b, not M4.) M7a decides, from
   `index-crate-isolation-ceilings.go`, how `testing`-gated items are counted, and sizes the `testing` export list
   against that.
8. **Event payloads: decided by the M0c spike.** 22 payload structs derive `tauri_specta::Event` today (19 in
   `types/events.rs`, plus `OperationsChanged` in `manager/wire.rs`, `VolumesBusyChanged` in `status_cache.rs`, and the
   staging-leftovers notice in `in_flight_sweep.rs`), and the engine BUILDS most of them; the orphan rule forbids the
   app from implementing `tauri_specta::Event` on a crate's type. The candidate is a hand-written `specta::Type` impl on
   an app-side wrapper (§ M0c); the fallback is the listing's app-side envelopes plus an M5c.
9. **`OperationId` newtype at the public API and in every seam trait**, wrapping at the IPC boundary so `bindings.ts`
   doesn't change. Internals adopt it where a milestone already touches the signature; a full internal sweep is a
   follow-up issue.
10. **What stays app-side:** `paste_clipboard.rs` (it composes `clipboard` payload types with the engine's instant-op
    API), `mutation_reply::broadcast_settled` (a Tauri emitter factory), `routing.rs`'s `expand_tilde` call (an IPC-edge
    concern), the S3 rename-cost policy (as a fact the engine returns, § "Seam design"), every wire envelope, the Tauri
    sink adapters, the Docker / virtual-MTP / ADB `backend_suites/` cells (§ "Tests"), and `transfer/real_image.rs` (the
    disk-image lane).
11. **`LocalPosixVolume` takes a `VolumeHost` at construction, like every other backend crate.** Its app reaches become:
    the pane-patch on a mutation → `host.listings().directory_changed` (the composer, unchanged), the watch-coverage
    oracle → a new `ListingHost::local_watch_coverage(path)` with a `WatchCoverage::None` default, and space + macOS
    metadata → code that moves INTO `cmdr-local`. ~60 constructor call sites (most in tests); a compile error at each is
    the point, because a silently detached host would quietly turn off the scan oracle's fast path in production.
12. **The runtime comes from the `VolumeHost`.** `Operations` spawns on `host.runtime()`; the builder takes no second
    handle, so the two can't disagree. With no host passed, the default is `VolumeHost::detached()`, whose runtime is
    `cmdr-fs`'s shared fallback, ❌ never a `Handle::current()` captured at build (that brings back the per-test-runtime
    orphaned-lane wedge `manager.rs:440-451` documents). The app passes `volume_host::host()`, whose runtime is
    `tauri::async_runtime::handle().inner().clone()`, so admitted ops keep running on the APP runtime.
13. **Behavior change 1 (lock poison) is elective**: #415 doesn't require it (the entries could re-path). The lead keeps
    it because `status_cache.rs:66,100,121` dropping the busy entry on poison lets Eject run mid-transfer. It's labeled
    elective in the commit body.

## Fresh survey (run 2026-10-16 on `dafc8fbbe`, spot-checked at `7d8054219` by review round 1)

Numbers below come from `grep` / `wc -l` over `apps/desktop/src-tauri/src`. "Production" means files not named `*test*`,
not under `tests/` or `copy_tests/`, and not `test_support`; inline `#[cfg(test)] mod tests` blocks inside production
files count as production here, and doc comments that name a path count too, so treat counts as ±5%.

### Sizes

- **App crate total**: 480,447 lines of Rust.
- **`write_operations/`**: 430 files. 64,158 production lines, 95,285 test lines (159,443, 33% of the app crate). 2,236
  `#[test]` / `#[tokio::test]` (of the app's 8,185), 315 of them in `backend_suites/` (13,810 lines, 47 files).
- **`operation_log/`**: 4,805 production lines, 5,068 test lines, 109 tests.
- **`volume/backends/`** (`LocalPosixVolume` + the local reader): 2,083 production lines (`local_posix.rs` 901,
  `local_posix/{reading 442, streams 409, scan 259, mutation 72}`, `mod.rs` 48), 1,668 test lines (`local_posix_test.rs`
  1,100, `local_posix_conformance_test.rs` 322, `local_posix/reading_test.rs` 246), 83 tests.
- **`cmdr-listing/src/collation.rs`**: 187 lines; its one crate tie is `crate::Inner` in a test helper.
- Internal path forms the move rewrites: 868 `crate::file_system::write_operations` self-references and 160
  `pub(in crate::file_system::write_operations…)` visibilities; 165 `#[path]` attributes (relative, survive `git mv`).

### Statics

52 `static` declarations across the two modules (the issue's number), 21 of them compiled into production:

- **Engine core → `Operations` fields (M6a)**: `WRITE_OPERATION_STATE` (`state/registry.rs:140`), `MANAGER`
  (`manager.rs:276`), `OPERATION_STATUS_CACHE` + `LAST_EMITTED_BUSY` (`status_cache.rs:29,199`).
- **The rest → `Operations` fields (M6b)**: `PREVIEWS` + `PREVIEW_SETTLED` (`scan_cache.rs:633,100`), `PROCESS_LEDGER`
  (`in_flight_temps.rs:198`; `Ledger` is already a handle type), `OUTAGES` + `NEXT_OUTAGE_ID`
  (`transfer/volume_outage.rs:423,446`), `REGISTRY` (`transfer/transfer_probe.rs:900`).
- **Stashed `AppHandle`s → `EngineEvents` (M5a)**: `OPERATIONS_APP` (`manager.rs:281`), `BUSY_APP`
  (`status_cache.rs:194`), `APP_HANDLE` (`in_flight_sweep.rs:663`).
- **Operation log → `OperationLog` fields (M2a)**: `JOURNAL` (`operation_log/mod.rs:44`), `ENABLED`, `UNRECORDED`,
  `UNRECORDED_LEN` (`operation_log/recording.rs:27,31,35`).
- **Production-compiled test seam**: `overwrite_aside_park.rs:28-30` (`ARMED`, `IS_PARKED`, `RELEASED`), read by every
  safe overwrite. Becomes a `testing`-gated field reached through the op's scope (M6b), so production carries no test
  atomics.
- **Allowed to stay**:
  - `CURRENT_TASK_PROBE` (`tokio::task_local!`, `transfer_probe.rs:859`): task-scoped, not process state.
  - The `cfg(test)` thread-local hooks in `journal_search.rs:226`, `transfer_sides.rs:277`, `durability.rs:308`,
    `transfer_probe.rs:104` (`STALL_ABORT_OVERRIDE`): each is read on the thread the test drives. Two
    (`transfer_sides::test_hook`, `durability::test_hook`) widen to `any(test, feature = "testing")` at the move.
  - `cmdr-local`'s live-space memo (`volumes/live_space.rs`, a `LazyLock<Mutex<HashMap>>` shared by the pane space
    poller and `expect_space_change`): a cache of an OS answer, not engine state, like `cmdr-fs`'s fallback runtime.
  - Until M7b–M7e, the `cfg(test)` process statics: `chunked_copy.rs:183-186` (`chunk_park`: `PARK_AFTER`, `PARKED_AT`,
    `IS_PARKED`, `RELEASED`), `scan_bridge.rs:434` (`OBSERVED_TICKS`), `operation_log/mod.rs:95` (`JOURNAL_TEST_LOCK`),
    and ~25 more in test files. They die with the guards in M7b–M7e or become fields of a test's own engine; M6b's DONE
    grep allows them by name.
- `cmdr-collation` has no static: the per-locale cache is `cmdr-listing`'s `Inner::collator()` (`collation.rs:97-123`,
  production code), which stays in `cmdr-listing`.

### `tauri` inside the three areas

49 non-comment production lines name `tauri` or `tauri_specta`; `volume/backends/` has none.

- `types/events.rs`: 19 `#[tauri_specta(event_name = …)]` payloads plus the `Event` import.
- `event_sinks.rs`: `TauriEventSink { app: AppHandle }` (`:161`), `TauriScanPreviewSink` (`:280`), the `Event` import.
  `TauriEventSink::emit_complete` / `emit_cancelled` / `emit_error` / `emit_settled` carry side effects: analytics,
  `mcp::terminal_ops::record`, a `warn!`, `manager().record_failure`, `volumes::expect_space_change`.
- `manager.rs`: `OPERATIONS_APP` + `init_operation_event_emitter` + `operations_app_handle()` (`:281-298`), the one
  `tauri::async_runtime::spawn(start())` that runs an admitted op on the APP runtime (`:452`), an `Event` emit (`:927`).
- `manager/wire.rs:193`, `status_cache.rs:184` (`VolumesBusyChanged`), `:194-252` (`BUSY_APP`, emitter).
- `in_flight_sweep.rs`: three `tauri::async_runtime::block_on` / `spawn_blocking` (`:61,73,106,110`), `APP_HANDLE`
  (`:663-679`).
- `rollback.rs` (the engine's executor glue): `AppHandle<R>` threaded through `dispatch_rollback`,
  `dispatch_inverse(_reported)`, `undo_operations`, `read_undoable_rows`, only to reach
  `app.try_state::<OperationLogWriter>()` (`:279-543`).
- `scan_preview.rs:82` (`app: AppHandle`), `mutation_reply.rs:22,160` (`broadcast_settled`), `scan_bridge.rs:360`,
  `transfer/transfer_probe.rs:1035` (`async_runtime::spawn`).
- `operation_log/mod.rs:27,219` (`start(app)`, `app.manage(writer)`), `operation_log/retention.rs:13,26,28,37`
  (`async_runtime::spawn` + settings through `app`).

### Spawns (production)

- `tauri::async_runtime::*`: 7 (above).
- `tokio::spawn`: `scan_preview.rs:116`, `transfer/retry.rs:273`, `archive_edit/fresh_compress.rs:630`,
  `mutation_reply.rs:133` (driven by five `start_paused` tests in `mutation_reply_tests.rs:29-114` and
  `strategy_yield_tests.rs:149`; § M4), `scan_watchdog.rs:170`, plus test-module ones in `operation_intent.rs`.
- `tokio::task::spawn_blocking`: `mod.rs:413` (every managed op's handler), `unblocked_removal.rs:65,69`,
  `rename.rs:246,577`, `rename/bulk.rs:179`,
  `archive_edit/{remote.rs:119, fresh_plan.rs:447, compress.rs:66, routing.rs:107, fresh_zip.rs:293, engine.rs:103}`.
- Dedicated threads (stay): `in_flight_temps.rs:558`, `in_flight_log_writer.rs:67`, `archive_edit/fresh_zip.rs:341`,
  `operation_log/writer.rs:207` (named `Builder`s), and bare `std::thread::spawn` in `cancellable.rs:24,40,70`,
  `scan_preview.rs:130`, `scan_preview/local.rs:92`, `transfer/real_image.rs:88,292`. M6b gives the bare ones names.

### App reaches out of the three areas (production), grouped by target

Total: 331 `crate::…` references out of `write_operations/` + `operation_log/` that aren't self-references, plus 96 from
the engine into `operation_log`. Kinds: **(a)** moves into the crate, **(b)** is or belongs in `cmdr-fs` / `cmdr-local`,
**(c)** stays in the app behind a seam.

- `crate::file_system::volume::*`: 151. 123 are `cmdr_fs::volume` re-exports (`Volume`, `VolumeError`,
  `ListingProgress`, `CopyScanResult`, `WriteMode`, `LaneKey`, `StreamLength`, …): **(b)**, re-pointed at `cmdr_fs` in
  M4. The real 28: `volume::manager::get_volume_manager()` (54 call sites: `.get` 25, `.resolve` 5,
  `.path_is_inside_archive` 2, `.on_volume_arrival` 1, `.forget_archive_index` 1, plus bound `manager`/`vm` variables):
  **(c)** `OperationVolumes`. `LocalPosixVolume`, `rename_local_exclusive`, `rename_volume_error`, `get_single_entry`,
  `patch_listing_after_local_mutation`: **(b)** `cmdr-local`.
- `crate::ignore_poison`: 49. **(b)**, already `cmdr_fs::ignore_poison`.
- `crate::operation_log::*` from the engine: 96 (`types` 68, `new_operation_id` 13, `rollback` 6, `writer` 2, `store` 2,
  `capture` 1, `journal_note_*` 2, `query` 1). **(a)** after M2b they're `cmdr_operation_log::*`; the `journal_*` free
  functions become methods reached through the op's scope in M6a.
- `crate::downloads::note_pending_write_for_cmdr`: 27 sites in 15 engine files (plus `commands/rename.rs` and
  `native_drag/fulfillment.rs` app-side). **(c)** `OwnWrites`. Called per file landing, before the syscall.
- `crate::log_error!`: 12 (`overwrite.rs` ×4, `transfer_tree/mod.rs` ×2, `transfer/copy/mod.rs:764`,
  `transfer/volume/copy.rs:385`, `transfer/volume/copy_concurrent.rs:319`, `transfer/transfer_probe.rs:616`,
  `manager.rs:458`, `status_cache.rs:255`). **(c)** typed `EngineEvents::error_report` (the `log-error-macro` check says
  crates raise errors as typed values the app re-raises).
- `crate::listing_host::listings()`: 9. `.authoritative_listing` ×7 (`scan.rs:91`, `scan_walker.rs:284`,
  `delete/walker.rs:407,737`, `scan_preview/volume.rs:307`, `conflict_preflight.rs:352`, `look_alike.rs:207`),
  `.refresh_archive_listings` (`archive_edit/engine.rs:79`), `.collator()` (`transfer_tree/order.rs:26`). **(c)** The
  first two are already `cmdr_fs::volume::host::ListingHost` methods (the app answers them in
  `listing_host/backends.rs`), so the engine reads them through the `VolumeHost` it holds; the collator is `NameOrder`.
- `crate::unregistered_volumes::why_unregistered`: 7 (`conflict_preflight.rs:23`, `mutation_error.rs:152`,
  `transfer/volume/transfer_error.rs:255`, …). **(c)** `OperationVolumes::unregistered`; the `Unregistered` enum moves
  to the crate's host module.
- `crate::analytics::{events::capture, item_count_bucket}`: 7 in `analytics.rs`. The completion half (called only from
  `TauriEventSink::emit_complete`) moves app-side with the sink; the instant-op half (`rename_used`, `folder_created`,
  `file_created`, `:97,116-117`) becomes `EngineEvents::instant_mutation_settled`. ❗ It WRAPS its driver today, so the
  in-archive early return still counts; the fact must fire from the same wrapper.
- `crate::index_host::index()`: 4 (`scan.rs:278`, `scan_preview/local.rs:72` `expected_totals`, `durability.rs:282`
  `dir_stats`, `journal_search.rs:32`), plus `cmdr_index::store::{IndexStore, resolve_path}` reads in
  `journal_search.rs` and `cmdr_index::{ExpectedTotals, ROOT_VOLUME_ID, store::normalize_for_comparison}` in
  `event_sinks.rs:21`, `create.rs:484`, `source_binding.rs:189`. **(c)** `OperationIndex`; the two pure helpers
  (`ROOT_VOLUME_ID` is `"root"`, `normalize_for_comparison`) move down to `cmdr-fs` or get engine-local copies, with a
  test pinning they agree.
- `crate::priority::transfers::note_transfer_{started,finished}`: 3 (`status_cache.rs:99,130`). **(c)**
  `EngineEvents::transfer_{started,finished}`.
- `crate::test_mode::*`: 6 (`effective_copy_throttle_ms` ×2, `_for` ×1, `e2e_scan_preview_delay_ms` ×2 in the engine,
  `effective_rollback_throttle_ms` in `operation_log/rollback.rs:456`). **(c)** `Pacing` (the rollback one through the
  engine's `RollbackRunner`, so the operation log gets no knob). `test_mode`'s overrides are runtime-mutable atomics
  (`test_mode.rs:61-92`), so this can't be a build-time value.
- `crate::file_system::staging::StagingTemp`: 4. **(b)** already mostly `cmdr_fs::staging`; the rest moves down.
- `crate::file_system::{validation, filesystem_kind, cloud_provider, index_provider::path_is_on_network_mount, linux_mounts}`,
  `crate::volumes::{is_mount_point, get_volume_space, expect_space_change}`: 12. **(b)** pure std / OS-probe code:
  `validation.rs` (198 lines, std only), `cloud_provider.rs` (309, std only), `filesystem_kind` detection, the
  mount-table probes (`volumes/mounts.rs::is_mount_point`, `linux_mounts.rs`), and the space reading
  (`volumes/nsurl.rs::get_volume_space`, `volumes/live_space.rs`) move to `cmdr-fs` (pure) or `cmdr-local` (OS probes,
  space). `expect_space_change` is called only from `TauriEventSink::emit_settled`, so it stays app-side with the sink.
- `crate::deadline::{race_detached, timeout_detached_within, Deadline, Raced}`: 2 files. **(b)** `deadline/mod.rs` is
  pure tokio + `cmdr_fs` + serde/specta (410 lines): moves to `cmdr_fs::deadline`.
- `crate::clipboard::*` (`paste_clipboard.rs`), `crate::commands::file_system::expand_tilde` (`routing.rs:52`),
  `crate::s3_costs` (`rename/validity.rs:14,197`), `crate::search::query::format_size` (`transfer/volume/copy.rs:539`, a
  log line), and doc-comment mentions of `crate::quit`, `crate::native_drag::fulfillment`, `crate::error_reporter`.
  `paste_clipboard.rs` moves app-side; `expand_tilde` moves to the IPC callers; the S3 cost check becomes a fact (§
  "Seam design"); `format_size` moves down or the log line uses a plain byte count.
- **Operation log's own reaches** (prod): `crate::sqlite_util` ×6 (already `cmdr_fs::sqlite_util`),
  `crate::ignore_poison` ×5, `crate::settings::load_operation_log_{enabled,retention_limits}` ×3 and
  `crate::config::resolved_app_data_dir` (all in `start` / `retention`: app glue), `crate::file_system::VolumeManager`
  (`rollback.rs:44`; the planner only calls `.get(id)`): **(c)** a one-method `VolumeLookup` trait in the crate.
- **`LocalPosixVolume`'s reaches** (prod): `crate::listing_host::listings()` (`local_posix.rs:453`
  `listings_for_local_path`, `reading.rs:216-222` `cmdr_listing::sort_entries` with the pane collator, `SortColumn`,
  `SortOrder`, `DirectorySortMode`), `crate::file_system::watcher::coverage_for_listings` (`local_posix.rs:467`),
  `crate::listing_host::directory_changed` (`mutation.rs:13`), `crate::file_system::macos_metadata::get_macos_metadata`
  (`reading.rs:73,419`; 93-line objc2 file, no other user), `crate::volumes::{get_volume_space, live_volume_space}`
  (`local_posix.rs:838,852`). `local_posix.rs:23,201,229,320,362` gate `injected_error` / `Volume::inject_error` on
  `feature = "playwright-e2e"` (today the APP's feature). Tests: `crate::test_support` (→ `cmdr_fs::testing`), and the
  watcher-lifecycle cell (`local_posix_test.rs:798-815`, stays app-side).

### Inbound callers (outside the three areas)

- **Production files naming the engine**: 49 (directly or through the `file_system/mod.rs:67-93` facade). The heavy
  ones: `commands/file_system/{write_ops.rs, volume_copy.rs, mod.rs, path_checks.rs}`,
  `commands/{rename.rs, clipboard.rs, multi_rename.rs, operation_log.rs, agent/*}`,
  `mcp/executor/{file_ops, queue, conflicts, async_tools, operation_log, quit}.rs`, `mcp/resources/operations.rs`,
  `mcp/terminal_ops.rs` (types only), `quit/mod.rs` (`list_operations`, `cancel_all_write_operations`,
  `abort_all_write_operations`, `flush_in_flight_temps`), `data_wipe/mod.rs`, `error_reporter/state_history/`,
  `native_drag/fulfillment.rs` (`register_external_volume_op` / `release_external_volume_op`), `multi_rename/run.rs`,
  `s3_costs/{mod,plan}.rs` (`ScanCostFacts`, `cached_cost_facts`), `agent/suggested_ops/bridge/` (wraps the sink),
  `ipc.rs` (22 event registrations), `lib.rs` (the init calls at `:364-365, :418, :425, :813`).
- **The operation log**: 28 files, mostly `commands/operation_log.rs`, `mcp/executor/operation_log.rs`, `settings/`,
  `agent/store/migrations.rs` (shares the migration-ladder shape; a `jscpd-rust` entry pairs the two files), `ipc.rs`,
  `test_mode.rs`, and the 13 MCP schema files that name its types. 15 app files use `TestJournalGuard`.
- **`LocalPosixVolume` constructors**: ~60 call sites; production ones are `file_system/mod.rs:150,195,203,220`,
  `volume/mount_registration.rs:86`, `network/smb_upgrade.rs:582`, `volumes_linux/watcher.rs:442`, `local_posix.rs:304`
  (its own `on_mount` re-creation), `native_drag/fulfillment.rs:278`, and the engine's
  `archive_edit/{copy_into.rs:259, remote.rs:214, fresh_compress.rs:98,484}`.
- **`operation-log-dump`** depends on the whole `cmdr` crate to call five `operation_log::store` read functions
  (`crates/operation-log-dump/src/main.rs:21`).

### Tests: which move, which stay

Move (they need only the crate plus fakes once the seams exist): the bulk of the engine's 2,236 tests, including 28 of
the 29 non-suite test files (all but `rename_merge_mtp_tests.rs`) that today reach `get_volume_manager()` /
`TestVolumeRegistration` / `listing_host` (they switch to a `FakeVolumes` registry and `cmdr_fs`'s
`RecordingListings::with_authoritative_listing`). All of `LocalPosixVolume`'s tests except the watcher cell. The
operation log's store, writer, capture, query, retention, and planner tests.

Stay app-side as integration cells (each named in the milestone that leaves it behind):

- `backend_suites/` (13,810 lines, 315 tests): Docker fixtures (SMB, SFTP, WebDAV, S3), `crate::volume_host::host()`,
  virtual MTP (`mtp_archive_test.rs`, which needs `cmdr/virtual-mtp` from `HostCargoLaneArgs`), the ADB fake + the
  downloads watcher (`adb_index_test.rs`). M7f moves them to
  `apps/desktop/src-tauri/src/operations_host/backend_suites/` where they drive the app's real `Operations`. They reach
  engine INTERNALS today
  (`super::super::{transfer::volume:: {copy_volumes_with_progress, copy_between_volumes, forward_volume_methods}, create::create_directory_core, rename::check_rename_validity_impl, delete_volume_files_for_test, seed_incoherent_scan_result_for_test, rename_managed, start_volume_copy, start_volume_move, delete_files_start, state::WriteOperationState, event_sinks::CollectorEventSink, types::…}`,
  `test_support::TestOperationGuard`, and `operation_log::{writer, store, rollback, capture, TestJournalGuard}`), so M7a
  sizes a `testing` export list of about 20 items for them. **Caveat for David's artifact:** the issue's "the 92k test
  lines would run without building Tauri" holds for all but these 13.8k lines, which still build the app.
- `transfer/volume/rename_merge_mtp_tests.rs` (virtual MTP), `transfer/real_image.rs` (the `disk-images` lane;
  `diskImageLaneTestAtoms` names its path), `transfer/volume/{copy_bench, copy_concurrency_bench}.rs` (ignored SMB
  measurement harnesses).
- The operation-log rollback cells that drive the engine's executor
  (`rollback/{control_tests, tests, blocked_items_tests, lifecycle_tests}.rs` and `rollback/test_support.rs`, which
  import `write_operations::rollback::Reversal`): app-side after M2b, then into `cmdr-operations` at M8b (the crate that
  has both halves).
- One cell per seam adapter proving the app's adapter reaches its subsystem (the `AppLeases` precedent from the
  listing): `OwnWrites` → the downloads ignore set, `OperationIndex` → `index_host`, `OperationVolumes` → the real
  registry, `EngineEvents` → `priority::transfers`.

### Checks and config that name these paths

These go silent, not red, when the code moves. Each is a landmine for the move milestones, and M3 hardens the first two.

- `write_operations/tests.rs::no_global_sync_or_spawn_async_sync_in_write_operations` walks
  `CARGO_MANIFEST_DIR/src/file_system/write_operations` and returns early on a `read_dir` error, so after the move it
  scans nothing and PASSES. It also excludes itself by file name. Once M3 makes it fail-closed, it goes RED the moment
  the directory moves, so its replacement lands in M8a, not later.
- `compress_estimate.rs:391` uses `CARGO_MANIFEST_DIR/src` as a corpus; after the move it measures a different tree.
- `desktop-rust-write-ops-agent-isolation.go` `continue`s when `file_system/write_operations` doesn't exist. Once the
  engine is a crate it can't name `agent::` (no `cmdr` dependency), so the check retires in M8b with its `registry.go`,
  CI, and `workspace-member-coverage.go:62` entries.
- `desktop-rust-derive-default-justified.go:135-139` walks `<member src>/file_system` (or all of `crates/cmdr-fs/src`),
  and `desktop-rust-probe-unwrap-justified.go:66` walks `<root>/file_system`. The new crates have no `file_system/`, so
  `LocalPosixVolume` (at M1c) and the engine (at M8a) leave both jurisdictions while the lanes stay green.
- `desktop-rust-discarded-outcome` is `AppTreeOnly` (`workspace-member-coverage.go:53-56`), so the engine's typed
  outcomes (`PauseOutcome`-style returns) lose it at M8a.
- `desktop-fixture-lane-coverage` is `AppTreeOnly` too: fine while the Docker suites stay app-side.
- `desktop-rust-disk-images.go:38` and `.config/nextest.toml:165` name
  `file_system::write_operations::transfer::real_image::` (`TestDiskImageLaneMatchesTheDiskImageNextestGroup` pins
  both). Module-path nextest filters (`:165`, `:420`) go red through `nextest-filter-coverage`, not silent.
- `.config/nextest.toml:205-263` SMB integration filters match by test-name substring, package-agnostic.
- `desktop-rust-tests.go:29-36` comment names `write_operations/backend_suites/mtp_archive_test`.
- `lock-poison-allowlist.json`: 20 engine exceptions (`delete/walker.rs` 1, `scan_cache.rs` 13, `state.rs` 1,
  `status_cache.rs` 5). `file-length-allowlist.json`: 16 entries; `jscpd-rust-allowlist.json`: 8.
- `module-cycles-allowlist.json` holds no `write_operations`, `operation_log`, or `backends` home today; the new crates
  must keep it that way.
- `apps/desktop/src-tauri/Cargo.toml:465-470` bans a self dev-dependency on `cmdr`: it links a second copy of the app,
  and the two copies' global, unmangled Objective-C classes (`CmdrPromiseDelegate`, `QuickLookDelegate`,
  `CmdrDragSource`, …) collide. M0a's bench reach-through must not use one.
- `docs/tooling/logging.md:197` recipe `RUST_LOG=cmdr_lib::file_system::write_operations=debug`: default log targets
  become `cmdr_operations::…` after the move. Explicit `target: "cmdr_lib::write_operations"` strings (5, in
  `scan_cache.rs`, `scan_preview.rs`, `transfer_tree/`) become `cmdr_operations`.

## Target layout

```
crates/cmdr-collation/        NameCollator and the collation rules (from cmdr-listing/src/collation.rs). ICU only.
crates/cmdr-local/            LocalPosixVolume, the std::fs reader (list_directory_core, get_single_entry), the
                              mutation → DirectoryChange mapping, rename_local_exclusive, macOS metadata, space
                              (statvfs + NSURL important-usage + the live-space memo), mount-table probes.
                              Features `testing`, `playwright-e2e` (forwarded from the app).
crates/cmdr-operation-log/    OperationLog (journal slot, recording gate, writer, store, query, retention loop),
                              the rollback PLANNER + `RollbackRunner` / `VolumeLookup` traits. Feature `testing`.
crates/cmdr-operations/       Operations (Arc<Inner>), host/ (the seams, their value types, no-op defaults, `testing`
                              fakes), testing/ (TestOperations, fakes, the app cells' export list), and today's
                              write_operations tree under src/. Crate-local clippy.toml.
apps/desktop/src-tauri/src/
  operations_host/            install(), operations(), one adapter per seam, the Tauri sinks, the event wrappers,
                              backend_suites/, the adapter cells.
  operation_log_host/         install(), start() (data dir, settings, open, retention), operation_log(), recording
                              live-apply, TestJournalGuard until M7e.
```

Inside `cmdr-operations`, keep today's tree shape (`transfer/`, `delete/`, `archive_edit/`, …): it is cycle-free under
`module-cycles` today, and `Inner` at the crate root (the listing's decision) keeps it that way. `types/` stays the
vocabulary floor that imports no sibling.

## Seam design

Each trait is `Send + Sync`, answered app-side by one adapter, defaults to a no-op under `detached()`, and has a
recording or scripted fake under `testing`. An adapter resolves its subsystem per call, never at construction (the
listing host's landmine: `install` order). Values captured at build (the `OperationLog`, the `VolumeHost`) are passed in
explicitly by `install` (§ "Decided", item 4).

### Cycle rules for seams and scopes

`state.rs:224-229` already warns that a call from `state` into `in_flight_temps` welds the two and `transfer_sides` into
one cycle; the `with_in_flight_ledger` precedent stays cycle-free only because `state` never CALLS a store. So:

- Every seam trait AND its value types (`OperationErrorReport`, `InstantMutationFact`, `UnregisteredVolume`,
  `ExpectedTotals`, `BufferedLeaves`, …) live in `host/` or `types/`, never in a module that calls the seam.
- `OpScope` holds only types from leaf modules (the stores' data structs, the seam traits from `host/`). `state.rs`
  never calls a store; the callers do, reading the store out of the scope.

### `cmdr-collation`

`NameCollator` and its rules, moved verbatim; `cmdr-listing` re-exports it. No traits, no static. `NameCollator::new`
(private today, `collation.rs:49`) becomes `pub`, for `cmdr-listing`, `NameOrder`'s `en` default, and `TreeOrder`'s
tests; count it in the surface. `cmdr-listing`'s per-locale cache (`Inner::collator()`, `collation.rs:97-123`,
production code) stays in `cmdr-listing`, in another file. The crate is justified by `cmdr-operations`' `TreeOrder`,
which builds `NameKey`s per item, whatever `cmdr-local` ends up needing.

### `cmdr-local`

No traits of its own. It takes `cmdr_fs::volume::host::VolumeHost` and uses `host.listings()`:

- `ListingHost::directory_changed(volume_id, parent, change)`: the existing method. `AppListings` already routes it
  through the composer (index → badges → panes), which is exactly where `patch_listing_after_local_mutation` sends it
  today.
- `ListingHost::local_watch_coverage(abs_path) -> WatchCoverage` (NEW, default `None`): the app answers with
  `listings().listings_for_local_path(path)` + `watcher::coverage_for_listings(ids)`, today's body verbatim. A pure
  in-memory read, called per directory a walker visits, which the dispatch rule allows.
- The reader's sort (`reading.rs:216-222`): characterize first. If every caller of `list_directory_core` re-sorts (the
  listing engine sorts after its pipeline), the backend sort goes and a test pins the callers' order. If not, the sort
  goes behind `ListingHost::sort_listing(&mut [FileEntry])`: one call per directory (the dispatch rule allows it), a
  bytewise default, and the app answers it with the pane collator. A collator can't be passed in: the callers are the
  `cmdr-fs` `Volume::list_directory` trait method (`reading.rs:66`), the tally variant, and the app watcher
  (`watcher.rs:553`), and `cmdr-fs` can't name `NameCollator` without putting ICU on every backend's build path. Either
  way `cmdr-local` never imports `cmdr-listing`.

### `cmdr-operation-log`

- `VolumeLookup { fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> }`: replaces `&VolumeManager` in the
  planner. App: the registry. The `OperationVolumes` adapter also answers it.
- `RollbackRunner` (exists): gains `fn pace_between_items(&self) -> Option<Duration>` (default `None`), so the planner's
  E2E throttle comes from whoever runs it.
- Retention: `OperationLog::spawn_retention(runtime: Handle, limits: Arc<dyn Fn() -> RetentionLimits + Send + Sync>)`;
  the closure reads settings live, as today.
- Recording: `OperationLog::set_recording(bool)`; decided once per op at `journal_open`, as today.
- **An op's journal is fixed at open.** `journal_open` returns an `OpJournal` (the `Arc<dyn OperationJournal>` or
  nothing) that the op carries in its scope for items, notes, and finalize. Today each call re-reads the slot, so an op
  opened before `open` filled the slot sends items and finalize to a journal with no row for it. Snapshotting makes an
  op wholly journaled or wholly not, which matches "recording decided once per op". (Not observable today in practice;
  it's the shape the scope makes natural, and M2a pins it.)

### `cmdr-operations::host`

1. **`OperationVolumes`: which volume serves an id or a path.** `get(id)`, `resolve(id, path)` (async →
   `ResolvedVolume`), `path_is_inside_archive(id, path)` (async), `forget_archive_index(path)`,
   `on_volume_arrival(listener)`, `unregistered(id)` (async → `UnregisteredVolume::{NotConnected, Gone}`). App:
   `AppOperationVolumes` over `VolumeManager` + `unregistered_volumes`. Default: nothing registered, `Gone`. ❗ An
   arrival listener runs INSIDE the registration (`volume/CLAUDE.md`): return at once, hand work to the runtime. ❗ "A
   site passing a path calls `resolve`, never `get`" (`volume/CLAUDE.md`): when M5d re-points the 25 `.get` sites, each
   one that passes a path becomes `resolve`, and each that stays `get` says why in one line.
2. **`EngineEvents`: engine-wide facts** (§ "Decided", item 6). `operations_changed(snapshot)`,
   `volumes_busy_changed(ids)`, `transfer_started(volume_ids)` / `transfer_finished(volume_ids)`,
   `staging_leftovers_kept(notice)`, `instant_mutation_settled(InstantMutationFact)`,
   `error_report(OperationErrorReport)`. App: `TauriEngineEvents` (Tauri emits, `priority::transfers`, PostHog,
   `log_error!`). Default: drops everything. The busy-set dedupe (`LAST_EMITTED_BUSY`) is engine state; the seam only
   hears changes. If M5a finds the busy set and the transfer pair fall out of one unfiltered "active volumes changed"
   fact naturally (both come from `status_cache.rs:99,130`, the busy set only drops `DEFAULT_VOLUME_ID`), it may merge
   them; M3's exactly-once pin stays either way.
3. **`OwnWrites`: "Cmdr is about to write here"** (the downloads watcher's ignore set). `announce(&Path)`. App: the
   downloads runtime. Default: nothing. Rule-as-type in M6d: `announce` returns an `Announced` token that the landing
   functions require.
4. **`OperationIndex`: what the drive index knows.** `expected_totals(&[PathBuf]) -> Option<ExpectedTotals>`,
   `recursive_size(&Path) -> Option<u64>`, `search_leaves(&Path, cap) -> BufferedLeaves` (today's
   `journal_search::enumerate_subtree_for_search` index read). Engine-owned value types. App: `index_host`. Default:
   `None` / an empty "uncovered" verdict (which downgrades search coverage, exactly what happens today with no index).
5. **`Pacing`: E2E throttles.** `copy_throttle_for(&Path) -> Option<Duration>`, `scan_preview_delay()`,
   `rollback_pace()`. App: `test_mode`. Default: none. Called per file in the copy loop today
   (`transfer/copy/mod.rs:533`); hoist to a per-op `PacingPlan` value if the bench shows any cost.
6. **`NameOrder`: the collator the panes sort with.** `collator() -> Arc<NameCollator>` (a `cmdr-collation` type). App:
   `listings().collator()`. Default: the `en` collator. Called once per walk.

The S3 rename cost is a FACT, not a seam: the engine returns the `ScanCostFacts` inside `RenameByMove`
(`rename/validity.rs:200`), and the app's IPC layer sets `costs_something` with `s3_costs::estimate_rename`. That drops
the `data_dir` parameter the engine shouldn't need and the two-way `s3_costs ↔ engine` tie. M5e verifies
`RenameByMove`'s consumers first; if the flag is needed inside the engine before the IPC return, it becomes a one-method
seam instead and the commit says why.

Values, not traits: the `VolumeHost` (its runtime is the engine's runtime; its `listings()` answers the fresh-listing
oracle and archive-pane refresh; every engine-minted `LocalPosixVolume` gets it) and the `OperationLog` handle. The data
dir arrives through `claim_ledger`, not the builder.

`Operations::builder(volumes, events)` takes the two seams nothing works without (an engine with no volumes writes
nothing, one with no events leaves every dialog on "Copying…", so forgetting either should not compile). Everything else
is optional; `Operations::detached()` defaults all of it for tools and benches.

**The ledger: one degrade rule, used word for word in M6b and invariant 32.** The installed engine's in-flight ledger is
persisted whenever a data dir resolves; when none does, it runs unpersisted and raises `error_report`, exactly as today
(`lib.rs:349` skips the sweep and the engine falls back to `PROCESS_LEDGER`). That is the only unpersisted production
path. A handle that never called `claim_ledger` (a test engine, `detached()`, a losing duplicate) records into memory
only and never reads, truncates, or sweeps a ledger file. `claim_ledger(data_dir) -> SweepHandle` consumes the handle's
one claim (a second call is `Err(AlreadyClaimed)`), so the type makes "only the installed engine sweeps" true without a
file lock; `instance_lock.rs` already refuses a second process whenever its lock can be taken (§ "Loud rules").

**Per-op reach.** Deep code reaches engine state through an `OpScope` built at registration: `Arc`s to the sub-stores it
needs and never an `Arc<Inner>` (the registry holds the states, so that would be a cycle). M6a's scope carries the
status cache, the op's `OpJournal`, and the seams (`OwnWrites`, `Pacing`); M6b adds the in-flight `Ledger`, outages, and
probes as those leave their statics. A managed op's scope rides on its `WriteOperationState`, generalizing
`WriteOperationState::with_in_flight_ledger` / `Ledger::of(state)`. **An instant op has no `WriteOperationState`**
(`run_instant`, `manager.rs:503-533`, registers a record but builds no state; `create.rs` and `rename.rs` journal
through free functions, and `Ledger::of` falls back to `PROCESS_LEDGER`): `run_instant` builds an `OpScope` of its own
and hands it to the driver. ❌ A missing scope never falls back to a detached `OperationLog` or to a ledger other than
the installed engine's: that silently drops undo rows and the persisted aside records.

**Deferred starts don't own the engine.** A queued op's start lives in the manager (`manager.rs:420-427`), inside
`Inner`; capturing an `Arc<Inner>` there makes a test engine with a queued op never drop. The admission pass hands the
start the engine (`FnOnce(Operations) -> Future`), or the start captures a `Weak` and upgrades at admission.

## Public API (the shape M7a programs against, measured into ceilings at M8a)

`Operations` methods, grouped by caller. Keep today's verbs so the IPC pass-throughs read one-to-one:

- **Start an operation:** `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`,
  `start_volume_copy`, `start_volume_move`, `start_volume_compress`, `start_rename_by_move`, `start_bulk_rename`,
  `start_renames`, `compress_start`, `route_archive_copy_into`, `route_archive_move_out`, `dispatch_rollback`,
  `undo_operations`.
- **Instant mutations:** `create_directory_managed`, `create_file_managed`, `rename_managed`, the `*_replying` variants
  plus `reply_within` / `MUTATION_REPLY_DEADLINE`, `check_rename_validity`, `check_rename_permission_for_volume`.
- **Previews and pre-flight:** `start_scan_preview`, `cancel_scan_preview`, `get_scan_preview_totals`,
  `scan_volume_for_conflicts_within`, `cached_cost_facts`, `trash_routing_for_selection`, `held_in_another_spelling`.
- **The queue:** `list_operations`, `latest_operation_progress`, `cancel_operation(s)`, `cancel_write_operation`,
  `pause_operation` / `resume_operation` / `pause_all` / `resume_all`, `move_queued_operation`,
  `place_queued_operation`, `retry_failed_operation`, `dismiss_failed_operation` / `dismiss_all_failed_operations`,
  `operations_need_volume`, `resolve_write_conflict`, `continue_without_gone_sources`, `transfer_tree_view`,
  `busy_volume_ids`, `register_external_volume_op` / `release_external_volume_op`, `retain_failure` (§ "Decided", item
  5).
- **Lifecycle:** `cancel_all`, `abort_all`, `flush_in_flight_temps`, `claim_ledger` (consuming, once per handle; the
  bare `init_and_sweep_in_flight_temps` is gone), `render_live_transfer_dump`.

The exact list is whatever `rg` over the app's callers finds at M7a; this is the expected shape. Every submodule but
`host` (and gated `testing`) is private; the root's `pub use` list is the API.

## Behavior changes, called out

Each lands as its own commit with a red test first, and the commit body names it.

1. **Lock poison recovers instead of skipping (M6c, elective, § "Decided", item 13).** The allowlisted sites are
   `if let Ok(..) = PREVIEWS.write()` / `.read().ok()?` shapes that silently skip their work on poison. Two answers, by
   site:
   - **The preview map (`scan_cache.rs`, 13 sites)**: on poison, recover AND CLEAR the map, so every binding is gone and
     every delete rescans. ❌ Never recover into a map a panic interrupted mid-mutation (`:348-360` and `:404-420`
     mutate entries in place): `take_cached_scan_result` (`:429-433`) is the read that binds a delete's path set, and
     today's poison answer, a miss, is the fail-closed one (`delete/CLAUDE.md`: an unbound cache deletes the PREVIEWED
     tree, with no rollback). Clearing keeps it fail-closed. Red test: poison the lock mid-mutation; the next delete
     rescans its sources.
   - **The other 7 sites** (`status_cache.rs` 5, `state.rs` 1, `delete/walker.rs` 1): `*_ignore_poison`. A real fix for
     `status_cache.rs:66,100,121`: today a poisoned cache drops the busy entry, and Eject is then allowed mid-transfer.
     Never `.expect`.
2. **The `no_global_sync` source scan becomes a clippy rule (M8a)**: `disallowed-methods = ["libc::sync"]` in the
   crate's `clippy.toml`. `spawn_async_sync` is a deleted helper, so clippy can't name it; that half stays as a tiny
   fail-closed test (root exists, ≥300 files scanned). Not a behavior change in the app; listed because a test changes
   shape and its guarantee moves to a lint.
3. **Nothing else.** In particular: terminal-outcome recording stays in the app's sink adapter, the admitted-op spawn
   stays on the APP runtime (the `VolumeHost`'s, which IS the app runtime), and an engine event before `install` is
   dropped by the default seam exactly as today's unset `AppHandle` statics drop it. The intent race is fixed before the
   effort (§ "Precondition"). A pre-install refusal is not a change: today no op can start before setup, and the refusal
   reuses an existing wire variant (§ M6a), so `bindings.ts` doesn't move. The ledger keeps today's one unpersisted path
   (no data dir) and gains no lock (§ "Seam design", "The ledger").

## Rules that become types

- **"Register the destination with `note_pending_write_for_cmdr` BEFORE the syscall (renames: both ends)"** →
  `OwnWrites::announce(&path) -> Announced<'_>`; `overwrite::stage_and_land_file`, the rename helpers, and the
  volume-engine local-destination writers take an `Announced` for every path they create or rename. Forgetting the
  announce stops compiling. The per-file `dyn` call is the one sanctioned per-file seam.
- **"Arm `state.conflict_slot` with the QUESTION before emitting `write-conflict`"** → `ConflictSlot::arm` returns an
  `ArmedConflict` (a newtype over today's `WriteConflictEvent`), and `OperationEventSink::emit_conflict` takes only an
  `ArmedConflict`. The "dispatch mutex never spans a write" half stays a rule.
- **"`OperationIntent` is one `AtomicU8`; never `store(...)` it"** → `pub intent: Arc<AtomicU8>` (`state.rs:50`) becomes
  a private `IntentCell` whose only writers are `request_stop(StopMeans)` and `stop_for_teardown()`, wrapping the
  precondition's landed `transition_intent`. Teardown can only produce `Stopped`.
- **"A reversal never asks `is_cancelled`; it reads `StopMeans`"** → the reversal entry points take a `StopMeans` reader
  and no `&WriteOperationState`, so `is_cancelled` is out of reach. Verify the shape fits the executor first.
- **"Test op state hangs off a guard, never a `static`; never `cancel_all_write_operations()` from a test"** →
  disappears: every test builds its own `Operations` (M7b–M7e).
- **"Emit through `OperationEventSink`, never `AppHandle`"** → disappears: the crate has no `tauri`.
- **"Never `tokio::spawn`" / "never a global `sync(2)`"** → the crate's `clippy.toml` `disallowed-methods`.
- **"Every managed mutation journals by `op_id`; a VOLUME op passes its REAL volume id"** → `OperationId` at the journal
  API. The real-volume-id half stays a rule unless a cheap `ResolvedVolumeId` from `OperationVolumes::get` fits.

## Measurement method (M0b and M9 run it identically)

Every timed `cargo` command carries `--features cmdr/virtual-mtp`, the feature set every check lane builds with
(`cargo-workspace.go:291`); a plain `-p cmdr` flips the feature set, costs 20–92 s per flip, and evicts other sessions'
warm artifacts. Every edit is a real string change, never a `touch`. Medians of 5 unless noted. Before each run, record
`sysctl -n vm.loadavg`. Don't time a run that waited on the cargo build lock (another session's
`ResourceCargoBuildDir`).

The driver is a short shell script committed beside the note (`docs/notes/transfer-extraction-baseline.sh`): it
alternates a named edit, runs a command N times, and prints median / min / max with the load before each run. A script,
not a new tool: the index note ran the same method by hand.

What to time, before → after:

1. **Engine inner loop.** Edit the `log::debug!` string at `write_operations/transfer/copy/mod.rs:164`
   (`"copy_files_with_progress: starting …"`), then `cargo check` and `cargo test --lib --no-run` on what you're
   editing: `-p cmdr --lib` before, `-p cmdr-operations` after.
2. **App build after an engine edit.** Same edit, then `cargo build -p cmdr`, before AND after. ❗ This does NOT get
   cheaper: cargo rebuilds dependents on any upstream change, so an engine edit still recompiles `cmdr_lib`. Report it
   as measured (the index note saw +11%); the win is row 1.
3. **Operation log and LocalPosix inner loops.** Same as row 1 with a log string in `operation_log/writer.rs` and in
   `volume/backends/local_posix.rs` (after: `-p cmdr-operation-log`, `-p cmdr-local`).
4. **App edit.** A log string in `commands/file_system/write_ops.rs`, then `cargo build -p cmdr`.
5. **Clean build, deps warm.** Before: `cargo clean -p cmdr && cargo build -p cmdr`. After:
   `cargo clean -p cmdr -p cmdr-operations -p cmdr-operation-log -p cmdr-local -p cmdr-collation && cargo build -p cmdr`.
   Median of 3.
6. **Release.** `cargo clean --release && cargo build --release -p cmdr`, twice (not the whole workspace: that builds
   `index-query` and `operation-log-dump`, whose dependencies change); wall time and `stat -f%z target/release/Cmdr`.
7. **Test lanes.** `pnpm check desktop-rust-tests --fresh`, 3 runs (duration from the check-log CSV under
   `~/.local/share/check-runner/cmdr/`), plus the engine's own slice: before,
   `cargo nextest run -p cmdr --lib --features cmdr/virtual-mtp -E 'test(/file_system::write_operations::|operation_log::|volume::backends::/)'`;
   after, `cargo nextest run -p cmdr-operations -p cmdr-operation-log -p cmdr-local`.
8. **Criterion.** Before:
   `cargo bench -p cmdr --bench transfer_benchmarks --features cmdr/transfer-bench,cmdr/virtual-mtp -- --save-baseline before`,
   twice. M0b commits each case's `target/criterion/<case>/before/estimates.json` beside the script, since `target/`
   won't survive a `cargo clean` or a fresh worktree clone. After:
   `cargo bench -p cmdr-operations --bench transfer_benchmarks --features cmdr-operations/testing -- --baseline before`,
   with the committed estimates restored into `target/criterion` first. The crate bench calls the SAME five internal
   entry points with the same Criterion IDs (through the `testing` exports), so the comparison is like for like; a
   public-API case may be added as a new ID.
9. **Complexity.** The same `grep` commands both times, pasted into the note: statics (52 total / 21 production),
   production `crate::` reach-ins (331 + 96), non-comment `tauri` lines (49), `module-cycles` homes for these areas
   (none), `lock-poison` exceptions (20).
10. **Public surface, one metric both times**: distinct engine items the app names from outside the engine. Before:
    `rg -o 'write_operations::[A-Za-z_]+|operation_log::[A-Za-z_:]+' apps/desktop/src-tauri/src --glob '!**/write_operations/**' --glob '!**/operation_log/**'`
    plus the `file_system/mod.rs:67-93` facade's names, deduped. After: the same `rg` over `cmdr_operations::` /
    `cmdr_operation_log::`. Report `index-crate-isolation`'s enforced counts beside it, labeled as the enforced number,
    not the comparison.

**M9's build rows (1–7) are paired by default.** The effort runs for weeks, and Renovate moves deps and the toolchain
meanwhile, so M0b's numbers become a context record, not the comparison. M9 takes every build row twice, minutes apart,
on the same toolchain: once on the new tree, once on M0b's commit in a sibling worktree (`new-worktree.sh`, removed
afterwards). Both figures go in the note.

## Milestones

Each milestone is one executor's work, sized for one sitting. "Lanes" names exactly what it must leave green, run as
`pnpm check <lane> …` (plus `--fast` while iterating); no milestone runs a full `pnpm check` or `--include-slow` except
where it says so (§ "Loud rules", the lanes rule). Each executor reports in ≤350 words and asks itself "is this solid
AND elegant?" before reporting.

Order: M0a then M0b, strictly sequential, and M0b runs ALONE (its numbers mean nothing next to another build). M0c runs
in its OWN worktree (`~/.claude/scripts/new-worktree.sh m0c-spike`, torn down with
`remove-worktree.sh m0c-spike --force` once its "Spike result" is appended here): the session's pinned worktree can't
switch branches under M0a. M0c may overlap M0a only, never M0b. Everything from M1a on is sequential.

### M0a: Criterion benches for the hot paths, and the frozen must-knows

**Scope.**

1. A new bench `apps/desktop/src-tauri/benches/transfer_benchmarks.rs` (Criterion 0.8.1, the version `cmdr-index`
   already uses, so no new dependency). The reach-through: a bench compiles against the app as an EXTERNAL crate, so it
   sees neither `#[cfg(test)]` nor `pub(crate)`, and `file_system` is a private module (`lib.rs:77`). Add an app feature
   `transfer-bench` that exposes ONE gated module, `cmdr_lib::transfer_bench`, with thin wrappers over the real entry
   points, and declare the bench with `required-features = ["transfer-bench"]`. ❌ Never a self dev-dependency
   (`Cargo.toml:465-470`: a second linked copy of the app collides its Objective-C classes). `desktop-rust-clippy`
   builds with `cmdr/virtual-mtp` only, so `required-features` makes it skip the bench; lint it explicitly in DONE, and
   the lanes rule's per-milestone `cargo check --bench` keeps it compiling until M8b. The feature and module die in M8b,
   when the bench moves to `crates/cmdr-operations/benches/` (where a self dev-dependency IS safe: none of the four
   crates defines an ObjC class).
2. **Frozen must-knows.** Append § "Frozen must-knows" to this plan: every bold-lead or `❌` must-know line from the
   eleven `CLAUDE.md`s the loud rules name, quoted verbatim with its source path, extracted mechanically at this commit.
   The M9 conformance reviewer checks every quote against where it lives after the move.

Cases (each asserts its fixture actually exercised the path before timing, the index bench's "refuse to measure nothing"
rule):

1. `local_copy_small_files/1000`: 1,000 × 4 KiB files in 10 folders → a fresh destination per iteration (`iter_batched`,
   setup and teardown untimed), through `copy_files_with_progress_inner` with a no-op sink and no journal. Measures the
   per-file path: own-write note, ledger, landing, progress, the tree. APFS clones small files, so this is engine
   overhead plus metadata syscalls, which is what a seam regression would show in.
2. `local_copy_small_files_journaled/1000`: the same with a temp-DB `WriterJournal` installed, so journaling (the
   operation-log crate boundary) is on the measured path.
3. `stream_large_file/64MiB`: `InMemoryVolume` source → `LocalPosixVolume` destination through
   `copy_volumes_with_progress`, which runs the chunked `write_from_stream` path with `sync_data`. This is the per-chunk
   loop: the case that would catch a `dyn` call landing inside it. (A same-volume local large copy is an APFS clone and
   measures nothing.)
4. `delete_walk/5000`: `delete_files_with_progress_inner` over a 5,000-file tree recreated per iteration.
5. `scan_preview/10000`: the local scan-preview worker over a fixed 10,000-entry tree. Read-only and idempotent, so it's
   Criterion's best case; also covers `walk_dir_recursive` / `scan_sources`.

Set Criterion's `noise_threshold` per case; disk-bound cases on a shared Mac may not hold ±2%, so record the measured
spread rather than gating on a number. Fixtures live under a `TestDir` in the system temp dir.

**Intentions.** The runtime "before" has to exist before any code moves, through the same entry points the "after" will
call; and the conformance review needs a referee that nobody paraphrased.

**Landmines.** `test_support.rs`'s `COUNTING_ALLOCATOR` replaces the global allocator in test builds only; benches run
under the `bench` profile (thin LTO, inherits `release`), so allocator-sensitive figures don't mix with test-suite ones
(`docs/notes/index-extraction-baseline.md` § "Read this before comparing"). Some entry points need a
`WriteOperationState` and an op id: build them the way `test_support.rs` does, without registering in the global
registry (a bench must not depend on `WRITE_OPERATION_STATE`, which M6a removes). Update the `Cargo.toml` comment that
says "No `testing` feature of our own" to name `transfer-bench` and why it isn't a self dev-dependency.

**Test plan.**
`cargo bench -p cmdr --bench transfer_benchmarks --features cmdr/transfer-bench,cmdr/virtual-mtp -- --test` runs every
case once; `cargo check -p cmdr --lib --features cmdr/virtual-mtp` still builds with the bench feature off.

**DONE.** Bench committed (`perf(bench): …`);
`cargo clippy -p cmdr --bench transfer_benchmarks --features cmdr/transfer-bench,cmdr/virtual-mtp -- -D warnings` clean;
two consecutive full runs with their spread recorded (numbers go to M0b's note); the frozen list appended; lanes:
`desktop-rust-clippy`, `workspace-member-coverage`.

### M0b: The "before" numbers

**Scope.** Measurements only: § "Measurement method", rows 1–10, plus its driver script. Write the "before" half of
`docs/notes/transfer-extraction-baseline.md`, in the shape of `docs/notes/index-extraction-baseline.md`: the method next
to every number, machine, toolchain, commit, load. Record first:
`sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize`, `sw_vers -productVersion`, `rustc -V`,
`git rev-parse --short HEAD`. Survey machine (2026-10-16): Apple M3 Max, 16 cores, 64 GB, macOS 27.0.1, rustc 1.99.0
(`b940084d7`). Other sessions run on this Mac: if the load is above ~4, pause, or record the paired-re-run fallback.
Commit the Criterion `estimates.json` files (§ "Measurement method", row 8).

**Landmines.** `cargo test --no-run` varies most (the index note saw 23–30 s): give a range when the spread is wide.
Every timed command carries `--features cmdr/virtual-mtp`.

**DONE.** The note's "before" half and the script committed (`docs(notes): …`); lanes: the doc lanes (`docs-reachable`
needs a link to the note, from `write_operations/DETAILS.md` § "Testing bar" or `docs/notes/performance/README.md`).

### M0c: The event-wire spike

**Scope.** Decide § "Decided", item 8, with evidence, in its own throwaway worktree (§ "Milestones", order). Take ONE
payload the engine builds (`WriteSettledEvent`, small) and try: the struct without `tauri_specta::Event` in a stand-in
module; an app-side wrapper
`#[derive(Serialize, Clone, tauri_specta::Event)] #[serde(transparent)] #[tauri_specta(event_name = "write-settled")]`
with a HAND-WRITTEN
`impl specta::Type for Wrapper { fn definition(t: &mut Types) -> DataType { <Inner as specta::Type>::definition(t) } }`
(legal: the wrapper is local), registered in `ipc.rs`. Regenerate bindings and diff.

Why hand-written: in specta-macros `2.0.0-rc.24` every derive wraps the type in a `NamedDataType` named after the Rust
ident, and `#[specta(transparent)]` / `#[specta(type = X)]` only swap the inner type. So a derived wrapper either
exports a new `export type <WrapperName> = WriteSettledEvent`, or (reusing the ident) produces two named types with one
name, which the TS exporter rejects. The hand-written impl returns the inner type's `DataType::Reference`, which is also
what `register_event` requires (it PANICS on a non-reference type, `tauri-specta` `src/lib.rs:239-241`).

Record:

- Whether `bindings.ts` is byte-identical (the payload type name, the `makeEvent<…>('write-settled')` line, no extra
  alias). The events-object key comes from the event NAME (`writeSettled`), not the ident, so the wrapper's name is
  free.
- The registry is keyed by the wrapper's `TypeId`, so every emit must go through the wrapper: a small macro in
  `operations_host/events.rs` makes each of the 22 one line.
- The command-return case: `latest_operation_progress` returns `WriteProgressEvent[]` as a command type, not only an
  event.

**Intentions.** The biggest unknown in the effort, settled before anyone designs seams around it, so M5's sizing comes
from evidence.

**DONE.** A "Spike result" section appended to this plan (the shape chosen, the diff, the edge cases), and the plan's M5
section rewritten concretely from it, never left conditional: if the wrapper works, M5c is deleted and M5b converts the
payloads; if not, M5c is sized from the spike and moves BEFORE M5b (it needs nothing M5b delivers once M5a has rerouted
the engine-wide emits), so M5b's "names no `tauri`" DONE is reachable. The spike worktree is removed.

### M1a: `crates/cmdr-collation`

**Scope.** `git mv crates/cmdr-listing/src/collation.rs` into a new crate; `cmdr-listing` depends on it and re-exports
`NameCollator` and whatever else its callers name, so no app path moves. `Inner::collator()` (the per-locale cache,
`collation.rs:97-123`, production code) stays in `cmdr-listing`, in another file; `NameCollator::new` becomes `pub`.
Manifest (ICU deps move with it, keeping `icu_collator`'s `unstable` comment), member, `inputs.go` `rustMemberTrees`
row, `index-crate-isolation` guard + surface entry, `testing` feature only if something needs it, docs (`CLAUDE.md` +
`DETAILS.md`; the collation rules and "❌ never persist a `NameKey`" move from `cmdr-listing`'s docs; single-source).

**Intentions.** `cmdr-operations`' tree order needs the collator (and `cmdr-local`'s reader might); neither should pull
the listing engine for it.

**Landmines.** `cmdr-listing`'s ceilings may shrink (tightening is free). `cmdr-listing`'s manifest loses the ICU deps:
`cargo-machete` catches a leftover.

**DONE.** `cargo check -p cmdr-collation` builds alone; the listing's golden-order tests pass unchanged; lanes:
`desktop-rust-tests`, `desktop-rust-clippy`, `index-crate-isolation`, `workspace-member-coverage`,
`desktop-rust-cargo-machete`, the docs lanes.

### M1b: `LocalPosixVolume` stops reaching into the app (in place)

**Scope.**

1. `ListingHost::local_watch_coverage(abs_path) -> WatchCoverage` in `crates/cmdr-fs/src/volume/host/listings.rs`
   (default `WatchCoverage::None`, a `RecordingListings` knob), answered in `listing_host/backends.rs` with today's body
   from `local_posix.rs:446-468`.
2. `LocalPosixVolume` gains a `host: VolumeHost` field; `local_folder(name, root, host)` and
   `on_mount(name, root, mount, host)`. Production call sites pass `crate::volume_host::host()`; tests pass
   `VolumeHost::detached()` or a host with `RecordingListings`, chosen by what each test asserts.
   `listing_watch_coverage` asks `self.host.listings().local_watch_coverage(..)`; `notify_mutation` sends its
   `DirectoryChange` through `self.host.listings().directory_changed(..)`.
3. Move `file_system/macos_metadata.rs` under `volume/backends/local_posix/`, and the space reading
   (`volumes/nsurl.rs::get_volume_space`, `volumes/live_space.rs` with `expect_space_change`, the Linux `statvfs` path)
   beside it, with the app's `volumes` module re-exporting what it still calls (the space poller, `commands/volumes.rs`,
   `TauriEventSink::emit_settled`).
4. The mount-table probes the engine asks, so `cmdr-local` is complete when its ceiling is first measured:
   `volumes/mounts.rs::is_mount_point`, `linux_mounts.rs::{is_mount_point, is_network_filesystem_linux}`,
   `index_provider::path_is_on_network_mount`, `filesystem_kind::detect_filesystem_for_path`, beside the backend, with
   the app re-exporting what it still calls. The engine keeps calling them through today's paths until M4 re-points it,
   and `transfer_sides::test_hook` stays in front of the mount question.
5. The reader's sort (§ "Seam design" / `cmdr-local`): characterize, then drop it or put it behind
   `ListingHost::sort_listing`.

**Intentions.** By the end,
`rg 'crate::' apps/desktop/src-tauri/src/file_system/volume/backends -t rust --glob '!*test*'` names only `cmdr_fs`
re-exports, so M1c is a move.

**Landmines.**

- `crates/cmdr-fs/src/volume/` had unrelated edits by other agents at survey time; rebase before starting and keep the
  `listings.rs` change minimal.
- `volume_host::host()` before `volume_host::install` returns a host without the event channel; `init_volume_manager`
  runs after `install` (`lib.rs:259` vs `:369`), so production volumes get the real host. Verify the order didn't
  change.
- An engine-minted spool or upload volume (`archive_edit/*`) gets `volume_host::host()` for now; M5d switches it to the
  host `Operations` holds.

**Test plan.** Pins first, against today's code: `notify_mutation` on a `LocalPosixVolume` reaches the composer (index
apply + badge invalidation + pane patch), and `listing_watch_coverage` answers `None` with no listing and the watcher's
coverage with one (the second exists as `local_posix_test::test_listing_watch_coverage_flips_with_watcher_lifecycle`;
confirm and keep it).

**DONE.** The grep above is clean, outside inline `#[cfg(test)]` blocks (say how you checked); lanes:
`desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-clippy-linux` (slow, needed: the Linux `statvfs` and
`/proc/mounts` paths move and nothing else compiles them on a Mac), `file-length`, `desktop-rust-jscpd`.

### M1c: `crates/cmdr-local`

**Scope.** `git mv` the backend, its `local_posix/` children, `macos_metadata`, the space files, and their tests into
`crates/cmdr-local/src`; `Cargo.toml` with dependency comments (copy `cmdr-listing`'s style),
`[lints] workspace = true`, workspace member, `inputs.go` `rustMemberTrees` row (no `[package.metadata.cmdr] kind`:
`app` is the default, `cargo-workspace.go:181-184`, and every other library crate uses it), `index-crate-isolation`
guard + surface entry (no handle), features `testing` and `playwright-e2e = ["cmdr-fs/playwright-e2e"]`, with the app's
`playwright-e2e` list (`Cargo.toml:24`) gaining `"cmdr-local/playwright-e2e"` (an undeclared feature is an
`unexpected_cfgs` error; an unforwarded one silently drops E2E error injection), and the app's `[dev-dependencies]`
gains `cmdr-local = { path = …, features = ["testing"] }`. Call sites name the crate (per `backends/CLAUDE.md`'s
"imported by crate name at its call sites" rule). Docs: the local backend's content in `volume/backends/CLAUDE.md` +
`DETAILS.md` moves to `crates/cmdr-local/CLAUDE.md` + `DETAILS.md` (single-source), leaving pointers.

Widen `desktop-rust-derive-default-justified` and `desktop-rust-probe-unwrap-justified` to `crates/cmdr-local/src` (and
their `Why` strings at `workspace-member-coverage.go:48,52`), each with a fail-closed "scanned ≥ N files under each
root" assertion.

**Intentions.** The last backend leaves the app, and the engine can depend on a crate for every local primitive.

**Landmines.** The watcher-lifecycle cell (`local_posix_test.rs:798-815`) stays app-side (it starts the real FSEvents
watcher); give it a home beside `file_system/watcher_test.rs` and keep its nextest override at
`.config/nextest.toml:420` pointing at the new path. `rename_local_exclusive` is the ONE no-replace rename: it becomes
`pub` in the crate, and the engine and the in-flight sweep call it by crate name.

**Test plan.** Everything that passed in M1b passes, now in two test binaries; `cargo check -p cmdr-local` builds alone;
`cargo tree -p cmdr-local -e normal` shows no `tauri`; an E2E error-injection cell still injects.

**DONE.** Lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `index-crate-isolation`, `workspace-member-coverage`,
`desktop-rust-cargo-deny` (a new member), `desktop-rust-cargo-machete`, `desktop-rust-derive-default-justified`,
`desktop-rust-probe-unwrap-justified`, `file-length`, `desktop-rust-jscpd`, the docs lanes.

### M2a: The operation log owns its state (`OperationLog`)

**Scope.** In place, app-side.

1. `OperationLog` (an `Arc<Inner>`): the journal slot (`JOURNAL`), the recording gate (`ENABLED`, `UNRECORDED`,
   `UNRECORDED_LEN`), the writer once opened. `OperationLog::detached()` (empty, journals nothing),
   `OperationLog::open(&self, db_path) -> Result<…, OpenError>` (store migrate/recreate/refuse, spawn the writer,
   reconcile `rolling_back` ops, THEN publish writer and journal in one slot write; `Err(AlreadyOpen)` on a second
   call), `journal_open` → `OpJournal` (§ "Seam design"), the record/note/finalize methods on it, `set_recording`,
   `writer()`, `spawn_retention(runtime, limits)`.
2. New app module `operation_log_host/`: `install()` (builds the detached handle, sets the `OnceLock`, RETURNS the
   handle; called early in `setup`), `operation_log()`, and `start(app)` (data dir, settings, `open(..)`, retention).
   `commands/operation_log.rs`, the MCP tools, the settings live-apply, and `write_operations/rollback.rs` reach the
   writer through `operation_log()` instead of `app.try_state::<OperationLogWriter>()` / `app.manage`.
3. The rollback planner takes `&dyn VolumeLookup` instead of `&VolumeManager`; `RollbackRunner` gains
   `pace_between_items`, and the engine's runner answers it from `test_mode` (still a direct call; M5e makes it
   `Pacing`).
4. The engine's free-function calls (`crate::operation_log::journal_open(…)` etc.) keep working through thin
   `// shim: removed in M6a` functions that live in `operation_log_host/` FROM THE START, over `operation_log()`; the
   engine's call sites re-point to `operation_log_host::journal_*`. ❌ Not in `operation_log/`: a shim there calling the
   app accessor makes `operation_log ↔ operation_log_host` a cycle. `TestJournalGuard` moves to `operation_log_host/`
   too (`cfg(test)`) and keeps working by setting the global handle's slot under its lock, until M7e.

**Intentions.** Statics → handle state for the smaller engine first, so the pattern (late-opening slot, app accessor,
shims) is proven before the big one.

**Landmines.** `start` runs late (`lib.rs:813`); `install` must run before anything can call `operation_log()`. A read
before `install` gets a detached handle and a `log_error!` (a reader losing journal rows is the listing-style loud
degrade; the write engine's stricter rule is § "Loud rules"). Recording is decided ONCE per op at open: the gate moves
with no semantic change. `reconcile_rolling_back_on_open` runs before the writer is reachable, so a rollback can't be
dispatched into a reconcile.

**Test plan.** The operation log's own tests build their own `OperationLog` (no `TestJournalGuard` inside
`operation_log/`). New: two handles side by side, an op journaled in one invisible to the other; a second `open` is
`Err(AlreadyOpen)` and spawns no second writer; an op opened before `open` journals nothing, even after `open` fills the
slot mid-op.

**DONE.** `rg -n '^\s*(pub(\([^)]*\))?\s+)?static\s' apps/desktop/src-tauri/src/operation_log -t rust --glob '!*test*'`
finds only `cfg(test)` items; `operation_log/` names no `tauri` and no `operation_log_host`; lanes:
`desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-lock-poison`, `desktop-rust-sqlite-open-direct`,
`desktop-bindings-fresh`, `desktop-rust-module-cycles` (slow, needed: `operation_log_host/` is a new app module the
engine calls into and that reaches `settings` and `config`, the first edge of that kind).

### M2b: `crates/cmdr-operation-log`

**Scope.** `git mv` `operation_log/` (minus the app glue, now in `operation_log_host/`) into the crate; manifests
(`specta = "=2.0.0-rc.24"` pinned exactly: `types.rs`, `query.rs`, and `store/*` derive it, and the registry already
holds `rc.25`; `rusqlite` with the app's features so a standalone `cargo check -p cmdr-operation-log` stays honest; one
SQLite is already guaranteed by `libsqlite3-sys`'s `links` key), member, `inputs.go`, isolation guard + surface entry
(HandleType `OperationLog`), `testing` feature, `#![deny(missing_docs)]`. `crates/operation-log-dump` depends on
`cmdr-operation-log` instead of `cmdr` (drop the `cargo-machete` ignore; `docs/architecture.md:515` loses "the one dev
tool that depends on `cmdr`").

The journal shims and `TestJournalGuard` are already in `operation_log_host/` (M2a); only the guard is `cfg(test)`. The
crate exports what the app cells reach (`store`, `writer`, `capture`, `rollback` helpers) under its `testing` feature,
and the app's `[dev-dependencies]` gains `cmdr-operation-log = { path = …, features = ["testing"] }`. The
executor-dependent rollback cells stay app-side (§ "Tests") until M8b.

**Landmines.** `agent/store/migrations.rs` and the crate's `store/migrations.rs` are a `jscpd-rust` pair; re-path the
entry (or let the agent store reuse the crate's ladder type, if that's a clean one-liner; otherwise a follow-up).
`desktop-rust-sqlite-open-direct` must still scan the crate (it uses `ScannerRoots`; confirm the member kind is
included).

**DONE.** `cargo check -p cmdr-operation-log` and `cargo build -p operation-log-dump` build without `cmdr`; lanes:
`desktop-rust-tests`, `desktop-rust-clippy`, `index-crate-isolation`, `workspace-member-coverage`,
`desktop-rust-cargo-deny` (a new member), `desktop-rust-cargo-machete`, `desktop-rust-sqlite-open-direct`,
`desktop-bindings-fresh`, `desktop-rust-jscpd`, the docs lanes.

### M3: Characterize what the engine seams will cut through

**Scope.** Tests only, plus two allowed production changes: (1) extract the side-effect half of `TauriEventSink`
(`emit_complete` / `emit_cancelled` / `emit_error` / `emit_settled`) into a plain function both the sink and a test
call; (2) a `cfg(test)` recording hook inside `downloads::note_pending_write_for_cmdr` that notes, per call, the path
and whether it exists yet (the app has no `testing` feature). Zero behavior delta.

Pins (pin REALITY, not this plan; verify each new timing- or order-sensitive pin goes red when you break the thing):

- **Own writes are announced before they exist**, for every write path: local copy, same-FS move (both ends), cross-FS
  move, delete, trash, rename (both ends), mkdir, mkfile, bulk rename, archive-edit landing, and the volume engine's
  local destination (`transfer/volume/strategy.rs::note_pending_for_local_dest`,
  `transfer/volume/move.rs:: note_pending_for_local_volume`). Red check: move one announce after its syscall; it must go
  red on a count or an order, not only on the "existed yet" bit. M5e swaps the hook for the seam and must keep this
  green.

  The recorder can't be per thread: every managed op's handler runs on `spawn_blocking` (`write_operations/mod.rs:413`),
  and the instant ops and the volume engine's writers run in the blocking pool too, so a thread-local recorder on the
  test thread records nothing, and "every recorded call happened before the path existed" would pass on zero records.
  So: one process-wide `cfg(test)` recorder keyed by path prefix. Each test registers its `TestDir` root and reads back
  only the paths under it, so siblings can't fill each other's records. The hook goes BEFORE `with_watcher(..)`
  (`downloads/runtime.rs:257-258`), because tests run with no watcher installed. Every pin asserts a NON-ZERO expected
  count per path (renames: both ends), so a write path that stopped announcing fails.

- **The terminal side-effect matrix**, per outcome (complete for each op type, cancel, error, settle): which of the
  completion analytics category, `terminal_ops::record` (and its status), the `warn!`, `record_failure`, and
  `expect_space_change` fired, in what order relative to the emit.
- **Retained failures**: a failed op through the production sink path is retained, and the failure is recorded only
  after the record is gone.
- **The full settle order**: journal finalize → terminal event (or the safety net's) → `on_settled` (cache removal,
  lanes freed, next admitted) → `write-settled` (`mod.rs:417-463`; `settle_event_tests.rs:133` pins part of it). New
  cell: an undo queued behind an op, when admitted, reads that op's row already finalized and eligibility-computed.
- **Instant ops journal and record through the real stores**: a rename journals its row and records its aside in the
  persisted ledger (the cell M6a's "no detached fallback" rule keeps green).
- **Busy volumes**: `volumes-busy-changed` fires once per distinct change (the `LAST_EMITTED_BUSY` dedupe), and every
  `note_transfer_started` has exactly one `note_transfer_finished` on success, cancel, error, and a panicking handler.
- **`operations-changed`** fires on register, admit, and settle, with today's count per op.
- **The admitted op runs on the app runtime**, not the caller's: a caller whose runtime is dropped right after
  `spawn_managed` still sees its op settle.
- **The staging-leftovers notice**: a kept non-empty `.cmdr-staging-<op>` on a drive raises the notice; a Mac-internal
  one only `warn`s.
- **Fail-closed scanners**: `no_global_sync_or_spawn_async_sync_in_write_operations` asserts its root exists and that it
  scanned more than 300 files (red: point it at a missing dir). Same for `compress_estimate.rs:391`'s corpus
  assumptions.

**Landmines.** Use the existing `CollectorEventSink`; don't build a second collector. `analytics::events::capture` has
no test observer; pin the category the extracted function computes. Timing pins use `wait_until_async` and gates, never
sleeps (`desktop-rust-test-sleep`). Don't call `cancel_all_write_operations()` from a test: use a private
`WriteOperationRegistry`. The downloads recorder is keyed by each test's own folder (above), never a per-thread hook and
never one unkeyed list a sibling test can fill.

**DONE.** New pins green; red-first noted per pin in the commit body; lanes: `desktop-rust-tests`,
`desktop-rust-clippy`, `desktop-rust-test-sleep`, `desktop-rust-fixed-temp-dir`, `desktop-rust-no-hand-rolled-fixture`,
`file-length`.

### M4: Vocabulary moves down

**Scope.**

1. Re-point every engine path that is only a `cmdr_fs` re-export at `cmdr_fs` directly (the 123 `volume` items, 49
   `ignore_poison`, `staging`, `filesystem_kind` types), inside `write_operations/` and `operation_log/`, so later greps
   count only real ties.
2. Into `cmdr-fs`: `deadline/mod.rs` as `cmdr_fs::deadline` (its `TimedOut` keeps its specta name; app callers re-point;
   its doc says its `spawn_blocking` is ambient by design), `file_system/validation.rs`,
   `file_system/cloud_provider.rs`, the remaining `staging` pieces, `ROOT_VOLUME_ID` / `normalize_for_comparison` if
   `cmdr-index` can depend on the moved copy (else an engine-local copy with an agreement test).
3. Re-point the engine's mount-table and space calls at `cmdr_local` (the code itself moved in M1b), and
   `crate::search::query::format_size` (`transfer/volume/copy.rs:539`, a log line): use a plain byte count, or move the
   formatter beside `cmdr_fs::pluralize` if other crates want it.
4. App-side: `paste_clipboard.rs` moves to `clipboard/` (or `operations_host/`) and calls the engine's instant-op API;
   `mutation_reply::broadcast_settled` moves to the IPC layer; `routing.rs`'s `expand_tilde` call moves to its callers.
5. **Decide `mutation_reply.rs:133`'s spawn.** Its `StillRunning` continuation is a `tokio::spawn` that five
   `start_paused` tests (`mutation_reply_tests.rs:29-114`) and `strategy_yield_tests.rs:149` drive on the test's paused
   clock. Either it stays ambient by design (documented, with the crate clippy's `#[allow]` and a reason), or those
   tests build their engine on their own paused runtime. ❌ Not by loosening the tests. Record the choice here.

**Intentions.** The engine's boundary shrinks to the questions that genuinely need the app.

**Landmines.** App modules that re-exported the moved items keep a `pub use` only where many callers would otherwise
churn; prefer re-pointing. `cmdr-fs` is uncapped by design.

**Test plan.** Moved tests move unchanged apart from paths. `cargo check -p cmdr-fs` and `-p cmdr-local` build alone.

**DONE.**
`rg -n 'crate::(ignore_poison|deadline|search::|file_system::(validation|cloud_provider|staging|filesystem_kind|linux_mounts|index_provider)|volumes::(is_mount_point|get_volume_space)|clipboard|commands::)' apps/desktop/src-tauri/src/file_system/write_operations -t rust --glob '!*test*'`
is empty outside inline `#[cfg(test)]` blocks and comments (say how you checked); if a moved item raised a ceiling in
`cmdr-local`, re-measure it without headroom (approved, § "Decided", item 7); lanes: `desktop-rust-tests`,
`desktop-rust-clippy`, `desktop-rust-rustdoc`, `index-crate-isolation`, `desktop-bindings-fresh`.

### M5a: Seams I — the runtime and engine-wide events

**Scope.** Create `write_operations/host/` (still app-side) with `EngineEvents`, its no-op default, its recording fake
(`cfg(test)` until M8a), and its value types; the app adapter `operations_host/events.rs`. Until M6a, the seams and the
`VolumeHost` live in a temporary slot (`write_operations::host::installed()`, `// shim: removed in M6a`) that
`operations_host::install` fills.

1. `OPERATIONS_APP`, `BUSY_APP`, `APP_HANDLE` die; `operations_changed`, `volumes_busy_changed`,
   `staging_leftovers_kept` go through `EngineEvents`.
2. `priority::transfers` and the instant-op analytics (from the same wrapper that counts the in-archive early return) go
   through `EngineEvents`.
3. `tauri::async_runtime::{spawn, spawn_blocking, block_on}` → the slot's `VolumeHost::runtime()`. The in-flight sweep's
   `block_on` calls stay on their dedicated thread / blocking task, now `handle.block_on` (legal from a blocking-pool
   thread).

**Intentions.** The engine stops reaching for a stashed `AppHandle` or the Tauri runtime.

**Landmines.** The admission pass spawning on the APP runtime is load-bearing: the slot's `VolumeHost` must be
`volume_host::host()`, never one built with `Handle::current()` in `setup`. `in_flight_sweep`: "❌ never
`async_runtime::spawn`; neither entry point runs on a runtime worker" (`DETAILS.md` § "What the sweep does with each
kind"). M3's pins are the referee.

**DONE.**
`rg -n 'async_runtime|OPERATIONS_APP|BUSY_APP|APP_HANDLE' apps/desktop/src-tauri/src/file_system/write_operations -t rust --glob '!*test*'`
is empty; M3's pins green; lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-bindings-fresh`,
`desktop-rust-module-cycles` (slow, needed: the seam slot is the first `host/` module the whole engine reaches, and the
cycle rules for seam value types start holding here).

### M5b: Seams II — the Tauri sinks move app-side, errors become typed

**Scope.**

1. `TauriEventSink` and `TauriScanPreviewSink` move to `operations_host/` with their side effects (§ "Decided", item 5);
   `record_failure` becomes `retain_failure` on the engine's public surface.
2. `archive_edit::global_tauri_sink()` (which builds a `TauriEventSink` from the stashed handle) becomes an injected
   sink factory on the slot.
3. The 12 `log_error!` sites become `EngineEvents::error_report(OperationErrorReport::…)`, typed variants grouped by
   what the app does with them; the app adapter re-raises through `log_error!` with today's target and message.
4. The event payloads per M0c's decision (the wrapper macro, if it won).

**Intentions.** After this, `write_operations/` names no `tauri` and no `AppHandle`.

**Landmines.** `SuppressTerminalsSink` and the agent's `ProposalReportingSink` wrap the Tauri sink; their forwarding
must stay exact (`emit_sources_gone` must be forwarded, per the trait doc). `record_failure` releases the manager lock
before logging or emitting (`manager.rs:741-745`); `retain_failure` keeps that.

**DONE.**
`rg -n '\btauri(_specta)?\b|AppHandle' apps/desktop/src-tauri/src/file_system/write_operations -t rust --glob '!*test*'`
finds nothing outside comments; M3's pins green; lanes: `desktop-rust-tests`, `desktop-rust-clippy`,
`desktop-bindings-fresh`, `desktop-rust-log-error-macro`, `desktop-rust-discarded-outcome`.

### M5c: App-side envelopes (only if M0c rejects the wrapper; then it runs BEFORE M5b)

**Scope.** The listing shape: the 22 envelopes move to `operations_host/events.rs` with their derives; the engine
reports crate-side facts; the app converts; the collector sink asserts on the facts. M0c rewrites this section
concretely (size, any split) before M1a starts, or deletes it if the wrapper wins.

**DONE.** `desktop-bindings-fresh` zero diff; lanes: `desktop-rust-tests`, `desktop-rust-clippy`.

### M5d: Seams III — volumes and the engine's `VolumeHost`

**Scope.** `OperationVolumes` (default, fake, app adapter), answering `VolumeLookup` too; the 54 `get_volume_manager()`
sites and the 7 `why_unregistered` sites go through it, each path-passing `.get` site re-checked against "a site passing
a path calls `resolve`, never `get`". The engine reads the fresh-listing oracle and archive-pane refresh through the
slot's `VolumeHost.listings()`, and engine-minted `LocalPosixVolume`s get that host.

**Landmines.** `on_volume_arrival`: the in-flight ledger's listener runs inside the registration; keep it returning at
once and holding the ledger weakly (`in_flight_temps.rs:226-235`, already weak). A volume-borne sweep cell needs a
volume id nothing else uses (`DETAILS.md` § "Testing the in-flight temp ledger"). A `.get` turned into a `resolve` is a
behavior change only if the path crosses an archive or `.git` route: if any does, it's a fix, its own commit, red first.

**DONE.** No `get_volume_manager` or `why_unregistered` in production engine code outside inline `#[cfg(test)]` blocks;
lanes: `desktop-rust-tests`, `desktop-rust-clippy`.

### M5e: Seams IV — own writes, index, pacing, order, the rename-cost fact

**Scope.** `OwnWrites`, `OperationIndex` (with `journal_search`'s `IndexStore` reads moving app-side behind
`search_leaves`; `SEARCH_LEAF_CAP` and the coverage downgrade reasons stay in the engine), `Pacing` (the runner's
`pace_between_items` included), `NameOrder`, and the rename-cost fact (§ "Seam design"). One app-side cell per adapter.

**Intentions.** After this, `write_operations/`' production code reaches the app only through `host::installed()` (the
slot) and the `operation_log_host::journal_*` shims, both of which M6a removes.

**Test plan.** M3's own-writes pin, now over a `RecordingOwnWrites`, stays green unchanged in what it asserts.

**DONE.**
`rg -n 'crate::' apps/desktop/src-tauri/src/file_system/write_operations -t rust --glob '!*test*' | rg -v 'crate::file_system::write_operations|host::installed|operation_log_host::(operation_log|journal_)|crate::test_support|TestVolumeRegistration|listing_host::testing|^\S+:\s*//'`
is empty. The `rg -v` drops self-references, the two shims, comment lines, and the test-scaffolding names that inline
`#[cfg(test)]` blocks in production-named files use (`transfer/macos_copy.rs:473`, `transfer/copy_strategy.rs:305`,
`delete/cloud_trash.rs:336`, …; they go with M7b–M7e). Read any other hit: an inline-test hit is fine and listed in the
report, a production hit is not. Lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-bindings-fresh`.

### M6a: The engine owns its state, the core (`Operations`)

**Scope.** `Operations` (an `Arc<Inner>`), `OperationsBuilder`, `Operations::detached()`. The manager, the registry, the
status cache + busy dedupe become `Inner` fields; the temporary seam slot becomes the handle's fields; the operation-log
shim goes (the handle holds the `OperationLog` that `install` passed in). `OpScope` lands with the status cache, the
op's `OpJournal`, and the seams, for managed ops on their `WriteOperationState` and for instant ops from `run_instant`
(§ "Seam design"); M6b adds the rest. `operations_host::install(operation_log, …)` runs unconditionally, builds the
engine, and `OnceLock::set`s it.

**The pre-install refusal lives app-side and adds no wire variant.** A new `EngineNotInstalled` variant on
`WriteOperationError` / `MutationError` would change `bindings.ts` (both are exported tagged unions the frontend
switches on) and owe UI copy. So the crate knows nothing about installs: `operations_host::operations()` returns
`Result<&Operations, NotInstalled>`, and the IPC pass-throughs map `NotInstalled` to an existing variant the frontend
already handles (`IoError` for starters, the matching `MutationError` for instant ops; pick by what the dialog shows
best and say why) plus a `log_error!`, since the state is unreachable after `setup`. In the test binary, `operations()`
lazily builds a working engine, as `listing_host` does.

**Shims stay out of the engine.** For callers outside the engine, the old free functions stay as
`// shim: removed in M7a` wrappers, but they read an engine slot INSIDE `write_operations/` (the M5a `host::installed()`
pattern, now holding the `Operations`) that `install` fills. ❌ Engine code never names `operations_host`: a shim in
`write_operations/` calling `operations_host::operations()`, while `operations_host` names the engine's types, is a
`write_operations ↔ operations_host` cycle (and through the `file_system/mod.rs` facade,
`file_system → write_operations → operations_host → file_system`), the shape `file_system/CLAUDE.md` already warns
about.

**Intentions.** The engine's spine stops being process-global, with zero behavior delta.

**Landmines.**

- **`spawn_managed` takes `&'static self` today** because an admitted op's task outlives every borrow; the admission
  pass now hands the start the engine (or the start upgrades a `Weak`). No `Box::leak` survives in production, and a
  queued op never keeps a dropped engine alive.
- **Settle order is a contract**: journal finalize → terminal event → `on_settled` → `write-settled` via the guard's
  `Drop`. Threading the journal through the scope is exactly where finalize could slide into the guard's `Drop`; M3's
  undo-behind cell is the referee.
- Code inside the engine uses `self` / its scope, never `operations_host::operations()`: a method reaching for the app
  accessor would work in the app and break for every other consumer.
- Install order: `operation_log_host::install` → `operations_host::install` → the in-flight sweep (today
  `lib.rs:363-365`) → `init_volume_manager`. Pin it with a startup-order cell or a comment the check can't miss.

**Test plan.** Whole suite green through the shims. New: two engines side by side, an op registered in one invisible to
the other; `Arc::ptr_eq` between the engine's log and `operation_log()`; the IPC pass-through over an empty slot (a
local `OnceLock` the test builds, no cfg switch) refuses a copy with the mapped variant and writes nothing.

**DONE.** No `WRITE_OPERATION_STATE`, `MANAGER`, `OPERATION_STATUS_CACHE`, or `LAST_EMITTED_BUSY` static; no production
path reaches a detached `OperationLog`; `write_operations/` names no `operations_host`; lanes: `desktop-rust-tests`,
`desktop-rust-clippy`, `desktop-bindings-fresh`, `desktop-rust-module-cycles` (slow, needed: the handle, `OpScope`, and
the in-engine slot are the riskiest new edges of the effort).

### M6b: The engine owns its state, the rest

**Scope.** The preview cache (`PREVIEWS`, `PREVIEW_SETTLED`), the in-flight ledger (`PROCESS_LEDGER`), outages
(`OUTAGES`, `NEXT_OUTAGE_ID`), the probe registry (`REGISTRY`), and the `overwrite_aside_park` test seam become handle
or scope state (the `OpScope` gains the `Ledger`, outages, and probes); the bare `std::thread::spawn`s get names.

**The ledger is the data-safety half.** `Ledger::init_and_sweep` (`in_flight_temps.rs:510-541`) reads
`data_dir/STORE_FILENAME`, truncates it, and sweeps every record (deletes temps, renames asides back,
`in_flight_sweep.rs:414-438`). A second ledger on the same data dir would replay the first one's LIVE records and delete
its temps mid-copy, the failure `in_flight_temps.rs:183-187` forbids, which today's single static makes impossible. The
replacement is § "Seam design", "The ledger": `claim_ledger` is the only path that reads, truncates, or sweeps the file;
it consumes the handle's one claim; only the `install` winner calls it; ❌ never inside `build()`. A second process is
already `instance_lock.rs`'s to refuse, whenever its lock can be taken. The degrade rule is the one written there, word
for word: persisted whenever a data dir resolves, otherwise unpersisted with an `error_report`, and that's the only
unpersisted production path.

**Landmines.** Background loops hold the engine weakly (the probe watchdog already exits when its registry entry is
gone, `transfer_probe.rs:1035-1046`). Per-op tasks may hold a strong `Arc`; they end.

**Test plan.** A second `install` and a second handle that never claims each leave the ledger file byte-identical; a
second `claim_ledger` on one handle is `Err(AlreadyClaimed)`; no data dir means an unpersisted ledger plus one
`error_report`. M3's instant-rename-records-its-aside cell is the referee for "no other ledger": it runs green through
the installed engine.

**DONE.** No production path reaches a ledger other than the installed engine's.
`rg -n '^\s*(pub(\([^)]*\))?\s+)?static\s|LazyLock|OnceLock<' apps/desktop/src-tauri/src/file_system/write_operations -t rust --glob '!*test*'`
finds only the § "Statics" allowed list: the task-local, the `cfg(test)` thread-locals, the `cfg(test)` process statics
named there (removed in M7b–M7e), and `TransferTreeSlot`'s field-level `OnceLock`. Lanes: `desktop-rust-tests`,
`desktop-rust-clippy`.

### M6c: The lock audit and the poison change

**Scope.** Audit every `Inner` and scope lock: no sink, observer, or seam call while held, and no I/O under a lock the
UI path also takes. The listing found an index-storm stall this way; expect lock-across-I/O here too, though that's a
guess. Each finding is its own commit, named, red first where a test can show it. Then behavior change 1, both halves.

**Intentions.** The step the issue expects to find bugs in, kept apart from mechanical work so the fixes get attention.

**DONE.** Findings listed in the commit bodies (an empty audit says so); `lock-poison-allowlist.json` has no engine
entry; lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-lock-poison`.

### M6d: Rules become types

**Scope.** § "Rules that become types": `Announced`, `ArmedConflict`, `IntentCell` (over the landed
`transition_intent`), the `StopMeans`-only reversal entry points, `OperationId` at the journal and seam APIs (then
re-measure `cmdr-operation-log`'s ceiling without headroom, approved in § "Decided", item 7). Each its own commit.

**Intentions.** Every `❌` line these replace stops costing tokens every session and starts being a compile error.

**Landmines.** `Announced` must cover the volume engine's local-destination hooks, which skip non-local destinations by
asking `dest_volume.local_path()`: keep that skip. Don't widen `OperationId` past the touched signatures.

**DONE.** The doc lines each type retires are pointers or gone; lanes: `desktop-rust-tests`, `desktop-rust-clippy`,
`desktop-bindings-fresh`, `index-crate-isolation`.

### M7a: The app programs against the public API

**Scope.** Every app caller (§ "Inbound callers") calls `operations().<method>` / `operation_log().<method>`; the M6a
shims and the `file_system/mod.rs:67-93` engine facade go. The engine's root gets its `pub use` list (§ "Public API");
every submodule but `host` becomes private, so the compiler keeps callers on the list.

Size the `testing` export list now: what the app-side cells (`backend_suites/`, the adapter cells, the executor cells)
reach, about 20 items (the internals listed in § "Tests"), plus the bench's five entry points
(`copy_files_with_progress_inner`, `copy_volumes_with_progress`, `delete_files_with_progress_inner`, the local
scan-preview worker, and what the journaled case needs to install a `WriterJournal`), which M8b's crate bench calls.
Gated items count apart from the ceilings (`index-crate-isolation.go:251-254`). While the engine is app-side, each is a
`cfg(test)` `pub(crate)` re-export in a `write_operations::testing` module; M8a widens exactly this list to
`any(test, feature = "testing")` `pub`. Read `index-crate-isolation-ceilings.go` first for how gated items count, and
record the decision.

**Landmines.** `quit/mod.rs` calls `cancel_all` → `abort_all` → `flush_in_flight_temps` on a deadline; keep the order.
`native_drag/fulfillment.rs` registers an external op for drag-out's busy volume: keep it a public method, typed by
`OperationId`. `s3_costs` reads `ScanCostFacts` / `cached_cost_facts`: public types.

**DONE.** No `// shim` marker left in the app;
`rg 'write_operations::' apps/desktop/src-tauri/src --glob '!**/write_operations/**'` names only the public root and
`testing`; lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-bindings-fresh`,
`desktop-rust-discarded-outcome`.

### M7f: The backend suites move app-side, over the `testing` list (runs right after M7a)

**Scope.** `git mv` `backend_suites/` to `operations_host/backend_suites/` while the engine is still in the app crate,
and re-point every `super::super::{…}` internal they reach at `crate::file_system::write_operations::testing::…`. This
proves the M7a export list compiles and the suites pass before the crate move, so M8a's suite step shrinks to a path
rewrite.

**Landmines.** The fixture lane selects cells by `test(<prefix>)` name, package-agnostic: keep every test NAME. The
virtual-MTP cell (`mtp_archive_test.rs`) needs `cmdr/virtual-mtp`, which `HostCargoLaneArgs` carries.

**DONE.** No `backend_suites/` under `write_operations/`; lanes: `desktop-rust-tests`, `desktop-rust-clippy`,
`desktop-fixture-lane-coverage`, and `desktop-rust-integration-tests` once (slow, needed: these are the Docker suites,
and this milestone moves them).

### M7b–M7e: Tests own their engine

**Scope.** A `testing` module: `TestOperations` (an `Operations` over one fake per seam it reads back) and
`TestOperationLog`. Every engine test builds its own engine, in four slices, each green on its own:

- **M7b**: manager, state, and queue tests (`manager/tests/`, `state_tests.rs`, `status_cache_tests.rs`,
  `queued_sources_tests.rs`, `tests.rs`, the settle and admission cells).
- **M7c**: `transfer/` (local, volume, driver, probe, outage, retry).
- **M7d**: `delete/`, `archive_edit/`, `rename/`, `create/`, the scan and preview tests, the in-flight ledger and sweep
  cells.
- **M7e**: the operation log's `TestJournalGuard` users across the app and engine, then the guard types themselves:
  `TestOperationGuard`, `TestJournalGuard`, `private_manager()`, `failures::leaked_manager()`, `force_admission_pass()`,
  and the `one_test_per_process()` gating go. App-side cells share the app's one engine and keep unique ids.

**Intentions.** Two engines share nothing, so a test asserts on engine-wide state (a manager counter, the busy set,
`cancel_all`) without seeing anyone else's, and plain `cargo test` stops being a poisoner.

**Landmines.** A counter assertion that passed only because nextest forks per test
(`retained_failure_stays_hidden_until_the_record_settles`) becomes honest; don't "fix" it by re-adding a gate.
Everything stays `cfg(test)` until M8a. Order: M7a, M7f, then M7b–M7e.

**DONE** (after M7e).
`rg -n 'TestOperationGuard|TestJournalGuard|leaked_manager|private_manager|one_test_per_process' apps/desktop/src-tauri/src`
is empty; `cargo test -p cmdr --lib --features cmdr/virtual-mtp -- file_system::write_operations operation_log` (plain,
threaded, filtered so app tests outside the effort that rely on nextest's process-per-test don't count) passes; lanes
per slice: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-test-sleep`.

### M8a: The move

**Scope.** `git mv apps/desktop/src-tauri/src/file_system/write_operations crates/cmdr-operations/src` (minus what stays
app-side, § "Tests"); rewrite the 868 self-references and 160 `pub(in …)` visibilities; `Cargo.toml` with dependency
comments; member; `inputs.go` row; `index-crate-isolation` guard + measured surface entry with item-by-item rationale in
the crate `DETAILS.md` § "The public surface is capped"; `#![deny(missing_docs)]`; `testing` feature, with the M7a list
widened from `cfg(test)` to `any(test, feature = "testing")` `pub` (exactly that list, plus the two thread-local hooks
named in § "Statics"), and the app's `[dev-dependencies]` gaining
`cmdr-operations = { path = …, features = ["testing"] }`; `specta` pinned to the app's exact version. In the same
milestone, because each goes red or silent the moment the directory moves:

- The crate's `clippy.toml`: a copy of the root's (it REPLACES the root config, so `cognitive-complexity-threshold` and
  `allow-unwrap-in-tests` must be copied) plus `disallowed-methods` for `tokio::spawn`, `tokio::task::spawn`,
  `tokio::task::spawn_blocking`, `std::thread::spawn`, and `libc::sync`, lifted under `cfg(test)` the way
  `cmdr-listing/src/lib.rs:14-17` does. The `no_global_sync` test shrinks to its `spawn_async_sync` half, fail-closed
  (behavior change 2).
- `desktop-rust-derive-default-justified` and `desktop-rust-probe-unwrap-justified` widen to
  `crates/cmdr-operations/src` with fail-closed counts; `desktop-rust-discarded-outcome` widens to the engine crate (or
  records a `Why` for staying `AppTreeOnly`, decided here).
- `operations_host/backend_suites/` (moved in M7f) re-point `crate::file_system::write_operations::testing` →
  `cmdr_operations::testing`.

**Landmines.** `module-cycles`: compare the crate's homes with what `cmdr::file_system::write_operations` showed
(nothing); a new home is an edge to cut, not a number to add. `cargo tree -p cmdr-operations -e normal` shows no
`tauri`, `tauri-specta`, or `cmdr`, and no `rt-multi-thread` from this crate's own manifest.

**Test plan.** Everything that passed in M7e passes, now in more test binaries; `desktop-bindings-fresh` zero diff.

**DONE.** Lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-rustdoc`, `desktop-bindings-fresh`,
`index-crate-isolation`, `workspace-member-coverage`, `desktop-rust-cargo-deny` (a new member),
`nextest-filter-coverage`, `desktop-fixture-lane-coverage`, `desktop-rust-derive-default-justified`,
`desktop-rust-probe-unwrap-justified`, `desktop-rust-discarded-outcome`, `desktop-rust-error-string-match`,
`desktop-rust-lock-poison`, `file-length`, `desktop-rust-jscpd`, and `desktop-rust-module-cycles` (slow, needed: the
crate's homes are new and must show no tangle).

### M8b: After the move

**Scope.**

- Dependency pruning: the deps the engine alone uses leave the app manifest; let `cargo-machete` / `cargo-udeps` find
  them (`zip`, `tar`, `flate2`, `trash`, `filetime`, `xattr`, … are candidates, verify each).
- Retire `desktop-rust-write-ops-agent-isolation` (registry, `ci.yml`, `workspace-member-coverage.go:62`, its test,
  `checks/DETAILS.md`): the crate has no `cmdr` dependency, so it can't name `agent::`.
- The bench moves to `crates/cmdr-operations/benches/transfer_benchmarks.rs`, calling the SAME five internal entry
  points with the same Criterion IDs through the `testing` exports (`required-features = ["testing"]`, or a self
  dev-dependency, safe here since none of the crates defines an ObjC class), so `--baseline before` compares like for
  like; a public-API case may be added under a new ID. The app's `transfer-bench` feature and `cmdr_lib::transfer_bench`
  go.
- The operation-log executor cells (§ "Tests") move into `cmdr-operations`' tests.
- `real_image.rs`: stays app-side or moves with a dev-dependency on `cmdr-fs`'s disk-image fixtures; either way update
  `diskImageLaneTestAtoms`, its `_test.go`, and `.config/nextest.toml:159-165` together.
- Log targets: the five explicit `cmdr_lib::write_operations` strings → `cmdr_operations`.
- `file-length`, `jscpd-rust` entries re-path (only what the layout didn't already fix).

**Test plan.** `desktop-svelte-e2e-linux` once (copy, move, delete, conflict, undo flows), and the disk-image lane once.

**DONE.** Lanes: `desktop-rust-tests`, `desktop-rust-clippy`, `desktop-rust-cargo-machete`, `workspace-member-coverage`,
`file-length`, `desktop-rust-jscpd`, plus three slow ones, each needed here: `desktop-rust-cargo-udeps` (dependency
pruning is the change), `desktop-rust-disk-images` (its atoms change), and `desktop-svelte-e2e-linux` once (copy, move,
delete, conflict, and undo flows end to end after the move).

### M9: Docs, the "after" numbers, and close-out

**Scope.**

1. **Crate docs.** `crates/cmdr-operations/CLAUDE.md` (must-knows only, 300–400 words, module map, "the host is a
   value", the seam rule, the `cfg(test)` rule, the install-and-ledger rule; every surviving must-know from
   `write_operations/CLAUDE.md`, shortened where a type now holds it) + `DETAILS.md` (today's
   `write_operations/DETAILS.md`, rewritten for crate paths and the handle, plus § "The public surface is capped" and §
   "Driving the engine without Cmdr" showing `Operations::builder`). The subdirectory `C+D.md` pairs move with their
   code and get path fixes. `src/host/CLAUDE.md` + `DETAILS.md` (the seam list, what each replaces, where the app
   answers it, the cycle rules). Same for `cmdr-operation-log`, and a check of `cmdr-local`'s and `cmdr-collation`'s.
2. **App docs.** `operations_host/CLAUDE.md` + `DETAILS.md` (the adapters, install order, the sinks' side effects and
   why they live there), `operation_log_host/` likewise. `docs/architecture.md` § "Workspace crates" (one entry per
   crate; `operation-log-dump`'s line), the isolation sentence's crate list, `AGENTS.md`'s crate list (one line, mind
   `resident-doc-budget`), `docs/tooling/logging.md:197` recipe, `file_system/CLAUDE.md` + `DETAILS.md`,
   `volume/DETAILS.md`, `crates/cmdr-fs/src/volume/host/DETAILS.md` (the new `ListingHost` method), the frontend
   `apps/desktop/src/lib/file-operations/*` docs that cite `write_operations/DETAILS.md` sections, `docs/testing.md`,
   `scripts/check/checks/DETAILS.md` (retired check, widened scanners).
3. **The "after" half of `docs/notes/transfer-extraction-baseline.md`**, by § "Measurement method", including the honest
   row 2 (an engine edit still rebuilds the app) and the backend-suites caveat.
4. **Close-out** per the playbook: an adversarial conformance review agent against § "Frozen must-knows" and the
   invariants register below, and a docs-audit agent that reads every commit body and verifies every touched
   `CLAUDE.md`.
5. **The explainer artifact is the lead's job, not this milestone's.** The lead builds David's Claude artifact (like the
   `cmdr-listing` one, https://claude.ai/artifact/KZV59Bn7Vbjh5pePqz2xB2): the crate graph, the handles and their seams,
   one copy end to end, rules that became types, bugs fixed along the way (the intent race, whatever M6c finds), caveats
   (the 13.8k Docker-suite lines still build the app; an engine edit still rebuilds the app), and REAL numbers from the
   note (check time, test-build time, `rust-tests` lane time, release build, statics, reach-ins, `tauri` refs, cycles,
   lock-poison exceptions, public surface, Criterion deltas). Executors hand the lead the note.

**Landmines.** Describe current state only (no "used to live in the app"). `docs-section-refs` catches renamed `§`
anchors cited from elsewhere; `write_operations/DETAILS.md` sections are cited from the frontend docs and from
`docs/architecture.md`. Single-source: the seam rationale lives in the crate's `host/DETAILS.md`; the adapters' side
effects live only in `operations_host/DETAILS.md`.

**DONE.** Lanes: `claude-md-length`, `claude-md-details-sibling`, `docs-reachable`, `docs-dead-links`,
`docs-section-refs`, `docs-link-text`, `resident-doc-budget`, `claude-md-reminder`, then the effort's ONE full
`pnpm check --include-slow` (the gate every slow lane skipped mid-effort waits for); the note committed; the review and
audit agents report no open findings.

## Invariants register (what the conformance reviewer checks at the end)

§ "Frozen must-knows" (appended at M0a) is the complete, verbatim list; this register is the index of what matters most,
plus what this refactor adds. Carried from today's docs (where each now lives is M9's job; a type that holds one is
noted):

1. `spawn_managed` for copy/move/delete/trash/archive edits/rollbacks, `run_instant` for rename/mkdir/mkfile; blocking
   work runs in the blocking pool, so a starter returns an id before any I/O. An instant op's analytics wraps its
   driver, so the in-archive early return still counts.
2. An instant op emits no `WriteCompleteEvent` and refuses with a typed `MutationError`, never a sentence.
3. A spawned op reserves every lane it touches or waits Queued; the next admits on `on_settled`, never on `Drop`.
4. Intent transitions are atomic and monotonic (`IntentCell`); a teardown stops to `Stopped`, never `RollingBack`;
   Cancel keeps copied files, Rollback removes only what it still recognizes, verifying before each destructive act,
   never in a batch; only the `Drop` net is unconditional; Pause is orthogonal and cancel wins; a reversal reads
   `StopMeans`, never `is_cancelled`; nothing a user clicks reaches the tier-2 abort.
5. A loop parks where it checks cancel, through `stop_or_park_sync` / `_async` / `ScanPause`; parking on a person
   updates both the `human_wait` clock and `announce_human_wait`. A rollback's pause parks BEFORE the item is verified.
6. A conflict question is armed before it's emitted (`ArmedConflict`); the dispatch mutex never spans a write; an answer
   names its clash (`ConflictId`), and `AlreadyResolved` / `StaleAnswer` / `NoPendingConflict` stay distinct.
7. Crossing types needs a person's per-shape consent (`resolution_for_clash`, all three engines); a conditional is never
   reduced before it.
8. Events go through `OperationEventSink`; the settle order is journal finalize → terminal event → `on_settled` →
   `write-settled` (exactly once); a cross-FS move's last `write-source-item-done` wins.
9. Every local write lands through `overwrite::stage_and_land_file` (temp + rename, rename-aside when replacing); a
   non-overwrite landing refuses an occupied destination; every destination is announced to `OwnWrites` before the
   syscall (`Announced`), renames at both ends; only a `temp` is removable on sight.
10. Every managed mutation journals by `op_id` with the real volume id; an op is wholly journaled or wholly not; bulk
    rename journals each hop as it lands; a rotation temp never enters `in_flight_temps`.
11. A transfer captures both sides at start; a drive that left is the mount table's answer, never an errno.
12. Free space: `statvfs` first, NSURL only on a shortfall; a shortfall is the person's call; copy reserves
    `total_bytes`, never `dedup_bytes`.
13. `types/` imports no sibling; `LifecycleStatus` is the one lifecycle answer.
14. Every preview runs under a `ScanWatchdog` bounded by inactivity; whoever settles it claims the outcome.
15. A failed op is retained out-of-band; the failure is recorded only after the record is gone.
16. Delete order: files first, then directories deepest-first; delete and trash never `fsync` and never `sync(2)`
    (clippy); a `preview_id` alone never authorizes a path set (`take_cached_scan_result` binds it, and a poisoned
    preview map is cleared, never trusted); a top-level source's type is never resolved with `.unwrap_or(false)`.
17. Trash of online-only cloud content becomes a delete only for a named provider, all-or-nothing; trash refusals carry
    a typed `TrashRefusalKind` from the NSError domain and code.
18. The transfer driver never calls `transfer_one` past the skip set, a Skip resolution, or a cancel; the cancel check
    sits before any destructive call; leaf progress is high-water per leaf under one lock; every skip credits the bars
    and `note_skipped`.
19. Cross-volume copy goes chunk by chunk through `open_read_stream` / `write_from_stream`, stages on `.cmdr-tmp-*`, and
    retries per file only inside `stream_pipe_file`; outages are waited for; the stall watchdog stays gated on `Dead`
    AND `STALL_ABORT_AFTER`; every streaming phase announces itself to the probe; one `FileWindow` per operation, and a
    walker never holds a permit while it recurses.
20. A move's source sweep deletes the walk's ledger, never the tree; a symlink is a leaf to every move; a ledger entry
    carries identity, never an mtime; a move-back never overwrites an occupied source; a merged move is not
    rollbackable; a cross-FS move journals final paths; created-dir rows journal on every terminal path.
21. A merge never deletes or overwrites a destination file the source doesn't shadow (`safety_oracle.rs`); a listed name
    joins a destination only as a `ChildName`; only `cleanup.rs::remove_tree` recurses; an unknown is-directory is never
    guessed; the destination pre-check is skipped only for a dir THIS op created; a destination size is never
    fabricated.
22. Archive edits are managed, every existing-archive apply goes through `run_managed_edit`, routing is parent-aware, a
    remote archive is never edited in place (pull → apply → upload → swap, original untouched until the swap),
    `fresh_zip` never treats EOF as success, unrepresentable entries are skipped never lost and any skip suppresses the
    source delete, and move-out deletes only what durably landed.
23. The transfer tree is observation only, ≤64 B per node, ids assigned once, one lock per file boundary, never built
    for delete or trash, and every driver starts files in tree order.
24. Operation log: durable and migrating (append-only ladder, refuse downgrade), no custom collation, one writer thread
    (a second `open` is refused), typed tokens mapped only in `types.rs`, eligibility computed in capture, recording
    decided once per op, newest-first undo, rollback fails safe with `verify_snapshot` / `SkipReason` / `SkipTally`
    shared with the in-flight reversals, the planner plans and the injected runner acts, UUIDv7 op ids, the journal
    records what survives, retention prunes whole ops and never a `rolling_back` one, and reconcile finishes before the
    writer is reachable.
25. `rename_local_exclusive` is the one no-replace rename; `LocalPosixVolume::write_from_stream` `sync_data`s each file
    plus a best-effort parent fsync; local force-rename is atomic; progress callbacks are fed in `list_directory` and
    the copy scan; every mutation calls `notify_mutation`; a site passing a path calls `resolve`, never `get`.

Added by this refactor:

26. `cmdr-collation`, `cmdr-local`, `cmdr-operation-log`, and `cmdr-operations` have no `tauri`, `tauri-specta`, or
    `cmdr` in their trees, and each public surface is capped (`index-crate-isolation`).
27. Every app concern arrives through a seam or a value the host passes in; every seam degrades to a no-op and has a
    `testing` fake; an adapter resolves its subsystem per call; seam value types live in `host/` or `types/`, and
    `state.rs` never calls a store.
28. A seam is called per operation, per directory, or per file landing, never per chunk or per listing entry; the
    Criterion benches stay within their recorded spread of M0b or the delta is explained.
29. No seam, sink, or observer runs under an engine lock.
30. Engine state lives on the handle; the only statics are the § "Statics" allowed list; background loops and deferred
    starts hold the engine weakly or not at all.
31. Everything in `cmdr-operations` and `cmdr-operation-log` spawns on the `VolumeHost`'s runtime (clippy), admitted ops
    run on the app's runtime, and the documented ambient exceptions (`cmdr_fs::deadline`, `cmdr-local`'s `Volume`-method
    `spawn_blocking`, and `mutation_reply` if M4 chose so) say why where they live.
32. A write engine never runs detached in production: before `install`, the IPC layer refuses every start with an
    existing wire variant. The in-flight ledger is claimed once (`claim_ledger`), by the installed engine only; it is
    persisted whenever a data dir resolves, otherwise unpersisted with an `error_report`, and that's the only
    unpersisted production path. No production path reaches a detached `OperationLog`. A second process is refused by
    `instance_lock.rs` whenever its lock can be taken, as today (it continues unlocked on an I/O error).
33. The engine reports facts; wire envelopes, analytics properties, error-report routing, and words live app-side, and
    `bindings.ts` doesn't change.
34. The MCP terminal ring, completion analytics, the failure `warn!`, and the space-change hint are side effects of the
    app's sink adapter; `retain_failure` is the engine's.
35. Test scaffolding an app-side cell reaches is `cfg(test)` while app-side and `any(test, feature = "testing")` once in
    a crate; every test owns its engine.
36. An op id is an `OperationId` at the public API and in every seam; only IPC sees a `String`.
37. `module-cycles` shows no tangle in any of the four crates, and the app's allowlist didn't grow.

## Frozen must-knows

(Appended by M0a: every bold-lead or `❌` must-know line from the eleven `CLAUDE.md`s named in § "Loud rules", quoted
verbatim with its source path, at the M0a commit.)

## Review log

Round 1 (safety, mechanics, shape). Every finding not listed here was folded in above.

- **Safety H1 (intent race unconditional)**: accepted, and per the lead it's fixed on `main` before the effort (§
  "Precondition") rather than in M3a or M6d.
- **Mechanics M2 option (c) (`cmdr-local → cmdr-listing`)**: rejected; it adds the listing engine to every `cmdr-local`
  consumer. Superseded by `cmdr-collation` (shape M3), with the reader taking `&NameCollator` from there if the sort has
  to stay.
- **Mechanics H4 alternative (move the pure network suites into `cmdr-operations` now, with the backend crates as
  dev-dependencies)**: deferred to a follow-up issue. It needs `desktop-fixture-lane-coverage` widened past
  `AppTreeOnly` and per-package lane rewiring, which is scope this effort doesn't need to ship; M7f moves them app-side
  over the `testing` exports instead.
- **Shape M1 second half (use `VolumeHost::analytics()` for instant-op analytics)**: rejected, with the reason recorded
  in § "Decided", item 6: that seam's string-pair shape is for backend counters, and the bucket and property types are
  app policy. The first half (runtime from `VolumeHost`) is accepted.
- **Shape L3 (merge the busy set and the transfer pair into one fact)**: not required; left to M5a as "if it falls out
  naturally", with M3's exactly-once pin kept either way.
- **Shape L4 first half (no new tool)**: accepted as a script beside the note. Second half (lock poison is elective):
  accepted as a label; the lead keeps the change (§ "Decided", item 13).
- **Shape M5 (which decisions go to David)**: superseded by the lead's decisions; nothing remains open.
- **Mechanics L4 (`[package.metadata.cmdr] kind` not needed)**: accepted.
- Superseded by round 2: the M0a–M0c parallel run, the `flock`, the `EngineNotInstalled` variant, and M8a carrying the
  backend-suite re-point (now M7f).

Round 2 (adversarial, executor). Every finding not listed here was folded in above.

- **Adversarial H1 items 2–3 (add the ledger `flock` as behavior change 3, taken before `init_and_sweep` reads the
  file)**: rejected under the lead's simplest-safe ruling. The finding's premise, that nothing stops a second process
  (it searched for `single.instance`), is wrong: `instance_lock.rs` already takes an `flock` on
  `<data dir>/.instance.lock` at `lib.rs:287`, before the sweep, so two processes never share a data dir. In-process,
  `claim_ledger`'s consuming, once-per-handle claim plus "only the `OnceLock::set` winner calls it" makes a second sweep
  unrepresentable. The `flock` is gone instead. Items 1 (one degrade rule, same words everywhere) and 4 (`install` runs
  unconditionally) are folded in.
- **Adversarial H2 option (b) (accept a `bindings.ts` change for `EngineNotInstalled`)**: rejected for (a): the refusal
  lives in the app's accessor and maps to an existing variant, so the zero-diff rule holds.
- **Adversarial H3 option 2 (pull the facade's engine re-exports out at M6a)**: not needed; option 1 (an in-engine slot)
  fixes the cycle without moving M7a's work earlier.
- **Adversarial M2 option (b) for `cmdr-local`**: superseded by (a): the mount probes move in M1b, so `cmdr-local` is
  complete when first measured. (b) is kept for `cmdr-operation-log` at M6d and for anything M4 touches.
- **Adversarial M3 item 3 (a `cargo check --bench` in every DONE from M4 to M8a)**: folded in once, as a loud rule
  covering M1b–M8a, rather than repeated per milestone.
- **Executor L4 (a named `Operations::not_installed()` constructor)**: moot. The refusal moved app-side (adversarial
  H2), and the test builds an empty `OnceLock` of its own, so no constructor or cfg switch is needed.
- **Executor H1 (M0c in its own worktree)**: folded in; recorded here because it reverses round 1's "run M0c alongside
  M0a and M0b": M0c may overlap M0a only.
- Round 3 (cold read): all five findings folded in (the per-folder own-writes recorder with non-zero counts, `-t rust`
  on the DONE greps, the after-bench feature and the bench entry points in M7a's list, `instance_lock.rs`'s
  unlocked-on-I/O behavior stated as-is, and the precondition marked met at `a5b218a63`). Nothing rejected.
- **The lead's lanes ruling** replaced every mid-effort `--include-slow` and full `pnpm check`. Slow lanes now appear
  only where named with a reason (§ "Loud rules"). Dropped: `desktop-rust-tests-linux` from M1b, M6a, and M6b, and
  `desktop-rust-module-cycles` from M1a, M4, M5b–M5e, M6b–M6d (all covered by M9's run).
