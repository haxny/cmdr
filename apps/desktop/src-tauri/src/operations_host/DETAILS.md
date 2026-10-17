# Operations host: details

The app side of the write engine's seams. The engine declares what it needs in `file_system/write_operations/host/`
(`write_operations/DETAILS.md` § "The engine's host"); this module answers.

## Startup

`install(app, operation_log)` runs in `setup()` right after `operation_log_host::install()`, which follows
`volume_host::install`, and runs unconditionally. It builds the app's one `Operations` with `Operations::builder` over
`volume_host::host()` (the engine's runtime), a `TauriEngineEvents` holding the app handle, the operation log
`operation_log_host::install` returned (passed by value, so the engine's log IS `operation_log()`), the app's answer
to every optional seam (`app_answers`, shared with the test binary's `test_engine`), and a sink factory that builds a
`TauriEventSink`, and keeps it behind `operations()`. That's before anything can start an operation, before the
ledger claim, and before `init_volume_manager`. Every app caller reaches the engine through `operations()` and calls a
method on it; the engine's root `pub use` list is all it may name.

`claim_ledger(data_dir)` runs later in `setup()`, where the leftover sweep always ran: after the instance lock (no other
process holds this data dir) and before `init_volume_manager`, unconditionally. It claims the installed engine's
in-flight ledger through `operations()`, so a duplicate that lost the install is never the one claimed; a second claim
is refused and logged. With no data dir the engine runs the ledger unpersisted and reports it
(`write_operations/DETAILS.md` § "Who claims the ledger").

- **Decision: a call before `install` is refused, never run detached.** `operations()` answers `Err(NotInstalled)`
  and raises a `log_error!`. A start maps it through `refuse_not_installed` to the frontend's generic `IoError` (its
  dialog renders that without naming a file), an instant op through `refuse_instant_not_installed` to
  `ArchiveEditNotReady` ("Cmdr is still starting up"), a scan preview through `refuse_scan_preview_not_installed` to an
  immediate error event; a query answers what an engine with nothing running would (an empty list, nothing busy, an
  unknown id). No new variant reaches `bindings.ts`. Unreachable in the app, which installs at the top of `setup()`;
  the engine knows nothing about installs.

- **Decision: the runtime comes from the volume host, with no second handle.** Why: the engine and the backends it
  drives then share one pool, and the two can't disagree about which runtime is the app's. The admission pass
  depends on it being the app's (`write_operations/DETAILS.md` § "The admission pass spawns admitted ops on the APP
  runtime").
- **Decision: the sink factory is the engine's only way to a Tauri sink.** Why: an instant op routed into a zip becomes
  a managed archive edit the ENGINE starts, with no caller-built sink to hand it; and the MCP rollback tool is generic
  over the Tauri runtime, so it can't build the concrete `TauriEventSink<Wry>` from the handle it holds. Both ask the
  factory, and a test binary (no factory) gets `None` and refuses plainly.

## The per-op sinks (`sinks.rs`)

`TauriEventSink` (`OperationEventSink`) and `TauriScanPreviewSink` (`ScanPreviewEventSink`) turn each event into its
Tauri event. Every IPC command that starts an operation or a preview builds one and injects it. Engine and agent
wrappers sit in front of it (archive move-out's `SuppressTerminalsSink` withholds the inner extract's terminals; the
agent's `ProposalReportingSink` adds a write and forwards everything, the defaulted `emit_sources_gone` included), so a
side effect added here reaches the person only through what each wrapper forwards.

The four terminal emits carry app side effects in a pinned order (`terminal_effects.rs`, pinned by
`terminal_effects_tests.rs` with a recorder):

- `write-complete`: completion analytics (`analytics.rs`), then the MCP terminal ring, then the emit.
- `write-cancelled`: the ring, then the emit.
- `write-error`: the ring, the failure `warn!` (target `write_operations`, the one place the technical reason is
  written down; `write_operations/DETAILS.md` has the WARN-not-error decision), the engine's
  `OperationManager::retain_failure`, then the emit.
- `write-settled`: the free-space hint (`cmdr_local::live_space::expect_space_change`, macOS), then the emit.

The ring and the retained failure land BEFORE the frontend hears: MCP's `await operation_complete` answers the instant
the event arrives, and a backgrounded op that fails while the queue window is closed can still say why. Completion
analytics are categorical only (op kind, a count bucket, a bool), and an instant op has no completion category: it never
emits `write-complete` and reports an `InstantMutationFact` instead.

## The wire (`wire.rs`)

The engine's payloads (`write_operations/types/events.rs`, plus `OperationsChanged` and `MutationSettled`) derive only
`serde` + `specta::Type`; the Tauri event, with its wire name, is the app's. `wire_event!` gives each a newtype that
derives `tauri_specta::Event` with `#[serde(transparent)]`, `ipc.rs`'s `collect_events!` registers the newtypes, and
every emit site writes `wire::WriteProgress(event).emit(&app)`. A payload emitted unwrapped doesn't compile.

- **Decision: wrappers, not app-side mirror structs.** Why: the orphan rule forbids implementing `tauri_specta::Event`
  on another crate's type, and once the engine is a crate its payloads are foreign. A newtype is one line per payload;
  mirrors would be 20 structs plus conversions, and the collector sink would assert on a second shape (verified across
  a real crate boundary by the event-wire spike, commit `b10cc9821`, 2026-10-16: `bindings.ts` regenerated identical).
- **Decision: the wrapper's `specta::Type` is hand-written**, returning the payload's own definition. Why: a derived
  one, even with `#[specta(transparent)]`, exports `type WriteProgress = WriteProgressEvent` and renames the events
  object's generic, so `bindings.ts` changes. The hand-written one keeps it byte-identical, and a command returning the
  payload (`latest_operation_progress`) still names the payload itself.
- **Wire names live only here now**, one literal per wrapper. The payload's doc comments and `#[serde]` attributes still
  reach `bindings.ts` from the engine, so they stay part of the wire.
- A settle callback the engine hands back (`MutationSettled`) goes out through `commands/settled.rs::broadcast_settled`,
  which takes the wrapper constructor (`wire::MutationSettled`) as its `wire` argument.
- A pin: `wire::tests::a_wrapped_payload_reaches_the_wire_unchanged` mounts a mock app and checks the raw JSON under
  `write-settled` equals the payload's own.

## What each fact becomes

- `operations_changed` → the `operations-changed` event (a failed emit `warn!`s under `op_manager`).
- `volumes_busy_changed` → `volumes-busy-changed`, wrapped in `VolumesBusyChanged`, an event struct this module owns
  outright since the engine only reports the ids (a failed emit is a `log_error!` under `eject`).
- `transfer_started` / `transfer_finished` → `priority::transfers::note_transfer_started` / `_finished`.
- `staging_leftovers_kept` → `move-leftovers-kept` (a failed emit `warn!`s under `copy`).
- `instant_mutation_settled` → `rename_used` / `folder_created` / `file_created`, with the `initiator`, `target`, and
  `outcome` tokens `analytics/DETAILS.md` § "Starter event set" catalogs.
- `error_report` → `log_error!`, one arm per `OperationErrorReport` variant, each with the message and target the
  engine's code always raised it with. A report that had no explicit target keeps the engine module's path as a
  literal (`cmdr_lib::file_system::write_operations::overwrite`, …), so `RUST_LOG` recipes and the report dispatch's
  per-chain filters keep matching wherever the engine's code lives.
- `scan_wait_progress` → `write-progress` (a failed emit `warn!`s under `op_manager`).

## The volumes (`volumes.rs`)

`AppOperationVolumes` answers the engine's `OperationVolumes` (and so the rollback planner's `VolumeLookup`) from
`get_volume_manager()`, reached per call: the registry is where volumes come and go, so a host that captured it at
`install` would answer from a stale picture, and one built before `init_volume_manager` from nothing. `unregistered`
maps `unregistered_volumes::why_unregistered`'s answer onto the engine's `UnregisteredVolume`, one variant each.
`routes_over_its_parent` is `path_routes_over_its_parent`, which reads the git portal's switch, so it stays app-side
even though it's pure string work. Pinned by `volumes::tests::the_adapter_answers_from_the_live_registry`.

## Own writes (`own_writes.rs`)

`AppOwnWrites` answers `OwnWrites::announce` with `downloads::note_pending_write_for_cmdr`, which no-ops while the
watcher sleeps and for a path outside the Downloads root (`downloads/DETAILS.md` § "Cmdr-own-write hook contract"). It's
called beside a syscall on every file landing, so it stays one prefix-checked insert. Under `cfg(test)` the hook feeds
`downloads/own_write_recorder.rs`, which is how `own_writes::tests::an_announce_reaches_the_downloads_hook` pins that an
announce reaches the hook. The engine's own announce order is pinned against its seam, in
`file_system/write_operations/own_write_announce_tests.rs`.

## The drive index (`index.rs`)

`AppOperationIndex` answers `OperationIndex` from `index_host::index()`, reached per call (the index is built in
`setup()`, and an adapter that captured it could win its slot with an unconfigured one). `expected_totals` maps the
index's count onto the engine's own `ExpectedTotals`; `recursive_size` is `dir_stats`' `recursive_size`;
`search_leaves` is the honest-coverage read behind the operation log's search leaves (`cmdr-operation-log/DETAILS.md` §
"Search-leaf enumeration"): the Live gate, path resolution, the epoch gate (`enumerate_from_id`), and the cap, each
miss a typed downgrade. Its cells (the coverage honesty, the cap's edges, an ignored latency bench) run against a
hand-built index DB; `an_unindexed_volume_answers_uncovered` pins the adapter itself.

## E2E pacing (`pacing.rs`)

`AppPacing` answers `Pacing` from `test_mode`: the IPC overrides a spec sets, then the `CMDR_E2E_*` variables, then
nothing. Each read is live, since a spec changes a throttle mid-run, and a zero throttle reads as no pause (the seam
never hands back a zero-length sleep). The scan-preview delay is gated on E2E mode inside `test_mode`, so a stray
variable can't slow a real build. Pinned by `pacing::tests::the_rollback_pace_follows_test_mode`.

## Name order (`name_order.rs`)

`AppNameOrder` answers `NameOrder` with `listing_host::listings().collator()`, asked per call: the reading language can
change between two transfers, and the listing engine already keeps one collator per language, so a transfer's tree and
the panes rank names identically. Pinned by `name_order::tests::the_engine_sorts_with_the_panes_collator`.

## A test binary

Nothing installs in a test binary, and some engine pins read the app's real answers: the transfer-gauge cells in
`status_cache_tests` assert on `priority::transfers`. So the first read of the engine's slot builds `test_engine()`, and
`operations()` hands out that same engine: `TauriEngineEvents` with no app handle (events go nowhere; the gauge, the
analytics client, and the error reporter work as in the app), the real `AppOperationVolumes` (cells register their
volumes in the real registry) over `volume_host::host()`, whose runtime is `cmdr-fs`'s shared fallback, the test
binary's shared operation log (what `TestJournalGuard` fills), the same optional seams as the app (own writes reach the
downloads hook and its recorder), and no sink factory.
