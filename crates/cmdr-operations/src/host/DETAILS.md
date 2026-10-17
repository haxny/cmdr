# The engine's host details

Pull-tier docs for `crates/cmdr-operations/src/host/`: every seam's contract, where the app answers it, the runtime the
engine spawns on, and the cycle rules. Read this before any non-trivial work here: editing, planning, reorganizing, or
advising. Must-knows live in `CLAUDE.md`. The app's answers, and the side effects each one carries, are single-sourced
in `apps/desktop/src-tauri/src/operations_host/DETAILS.md`.

## The seams at a glance

Each line: the question the seam asks, its trait file, who answers it in Cmdr, and what it does with no answer.

- **The runtime** (a value, not a trait): the `VolumeHost` passed to `Operations::builder`. Cmdr passes
  `volume_host::host()`, whose runtime is the app's Tauri runtime. Default: `VolumeHost::detached()`, whose runtime is
  `cmdr-fs`'s shared fallback, ❌ never a `Handle::current()` captured at build (it would orphan every op admitted onto
  a runtime torn down later).
- **Which volume serves an id or a path**: `OperationVolumes` (`volumes.rs`), with the rollback planner's `VolumeLookup`
  as its supertrait. App: `operations_host/volumes.rs` over the `VolumeManager` and `unregistered_volumes`. Required.
- **Engine-wide facts**: `EngineEvents` (`events.rs`), its error reports as `OperationErrorReport` (`error_report.rs`).
  App: `operations_host/events.rs` (Tauri emits through `operations_host/wire.rs`, `priority::transfers`, PostHog,
  `log_error!`). Required.
- **One operation's events**: `OperationEventSink` (`operation_sink.rs`), handed to each start; one scan preview's:
  `ScanPreviewEventSink` (`scan_preview_sink.rs`). App: `operations_host/sinks.rs`, built at the IPC edge.
- **A sink for an op the engine starts itself**: `OperationSinkFactory` (`mod.rs`). App: the Tauri sink factory in
  `operations_host::app_answers`. Default: none, so an instant op routed into a zip refuses plainly.
- **"Cmdr is about to write here"**: `OwnWrites` (`own_writes.rs`), reached only through `EngineHost::announce` /
  `announce_on`, whose `Announced` the writers require. App: `operations_host/own_writes.rs` → the downloads watcher's
  ignore set. Default: nothing.
- **What the drive index knows**: `OperationIndex` (`index.rs`). App: `operations_host/index.rs` over `index_host`.
  Default: no totals, no size, and a `volume_not_live` search downgrade.
- **Deliberate slowness for automated runs**: `Pacing` (`pacing.rs`). App: `operations_host/pacing.rs` over `test_mode`.
  Default: `None` everywhere, which every real build answers too.
- **The panes' name order**: `NameOrder` (`name_order.rs`). App: `operations_host/name_order.rs` over the listing
  engine's collator. Default: English, built once on the answer.

## How the engine holds its host

The engine needs these from its host as named seams. The engine is a handle, `Operations` (`lib.rs`, built in
`handle.rs`): an `Arc<Inner>` holding the seams as one `EngineHost`, the operation log it journals into, the manager,
the registry of live states, and the status cache, so two engines share nothing. The app's answer is
`apps/desktop/src-tauri/src/operations_host/`, which builds the app's one engine in `setup()` with `Operations::builder`
(after `volume_host::install` and `operation_log_host::install`, before the leftover sweep, which it claims through
`operations_host::claim_ledger`). Every caller outside the engine reaches it through `operations_host::operations()` and
calls a method; the engine keeps no slot of its own, and nothing inside it names the app's accessor.

- **The runtime is the `VolumeHost`'s.** One handle for the engine and the backends it drives, so the two can't
  disagree. Everything the engine spawns goes there: the admission pass (`crates/cmdr-operations/DETAILS.md` § "The
  admission pass spawns admitted ops on the APP runtime"), every handler's and instant op's `spawn_blocking`, the scan
  preview's worker and watchdog, the stall watchdog, the arrival sweep's `spawn_blocking`, and the sweep's `block_on`s.
  The crate `clippy.toml` refuses `tokio::spawn`, `tokio::task::spawn{,_blocking}`, and `std::thread::spawn` (a thread
  the engine owns is a named `std::thread::Builder` one). A helper with no engine in reach takes the `Handle` as a
  parameter, and a test calling one directly passes its own (`Handle::current()`).
- **One documented exception: `mutation_reply.rs`'s late-settle continuation stays ambient.** `reply_or_hand_off` is a
  free function the app's IPC commands call with no engine, and it waits out `cmdr_fs::deadline::race_detached`'s own
  ambient spawn (that crate's documented exception), so the work and the task waiting it out share the caller's runtime;
  in production that's the app runtime either way. Moving only the continuation would split the pair for nothing, and
  would put the five paused-clock cells' continuation on a clock they don't drive. The site carries the reasoned
  `#[allow(clippy::disallowed_methods)]`.
- **The same `VolumeHost` answers the panes' questions and backs every volume the engine mints.** The fresh-listing
  oracle (`authoritative_listing`), the archive pane refresh, and the pane patch after a local create go through the
  op's `scope.host().listings()`; a spool, an upload reader, or a pull's scratch folder is a `LocalPosixVolume` over
  `scope.host().volume_host()`.
- **An operation reaches its engine through its `OpScope`** (`scope.rs`): the host's seams, the status cache, the
  engine's `EngineStores` (the scan previews, the in-flight ledger, the lanes' outage records, the transfer probes), and
  the op's `OpJournal` once it opens, built by the engine that runs it. `EngineStores` is defined at the hub beside
  `Inner`: each store's module also works on op states, so the scope naming those modules would circle back through
  `state`. A managed op's rides on its `WriteOperationState` (`state.scope()`); an instant op builds one beside
  `run_instant`. A scope holds parts of the engine, never the engine (the registry holds the states, so that would be a
  cycle), and ❌ a missing scope never falls back to another log. Entry points (starters, routing, the queue and by-id
  controls, pre-flight checks, rollback) are `Operations` methods or take the engine as a parameter; a deferred start
  uses the engine admission hands it. A scan preview's walk holds its engine (`run_scan_preview(&engine, …)`); the
  leftover sweep runs on the host its claim hands it; the probe watchdog and the preview watchdog hold their stores
  weakly, so a dropped engine stops them.
- **Which volume serves an id or a path goes through `OperationVolumes`** (`host/volumes.rs`): the registry's `get`,
  `resolve`, the archive-boundary questions, `find_by_root`, `forget_archive_index`, `on_volume_arrival`, and
  `unregistered` (why an id has no volume, as an `UnregisteredVolume`). Its supertrait is the rollback planner's
  `VolumeLookup`, so one answer serves both. The app answers it with `operations_host/volumes.rs`, which asks
  `get_volume_manager()` per call. ❗ A site passing a path calls `resolve`; each that stays on `get` says why in one
  line (the `.zip` file itself is a plain file on its parent; a lane key or a capability flag is a question about the
  drive, not the path; an archive gate upstream already routed the in-zip case).
- **Engine-wide facts go through `EngineEvents`**: operations changed, busy volumes changed, a transfer started /
  finished on volumes (indexing priority), staging leftovers kept, an instant mutation settled (analytics, as an
  `InstantMutationFact` of categories only), an error report, and the scan bridge's scanning-phase `write-progress` (one
  op's event, but the preview's worker that produces it holds no op's sink). Per-op events stay on `OperationEventSink`.
  Every method is required, is called with no engine lock held, and returns at once.
- **Error reports are typed facts** (`host/error_report.rs`): the engine never calls `log_error!` (the app's macro, at
  its crate root), it names what happened as an `OperationErrorReport`, grouped by what it means for the person: an op
  failed, a displaced original is stranded under its aside name, a transfer is stuck on a silent backend, or a contained
  bug. The app re-raises each through `log_error!` with the message and target (the engine's module path) it always had,
  so bundles, greps, and dispatch filters see no change.
- **"Cmdr is about to write here" goes through `OwnWrites`** (`host/own_writes.rs`), once per path BEFORE the syscall
  that lands it (`crates/cmdr-operations/DETAILS.md` § "Cmdr-own-write hook (downloads watcher)"). The one per-file seam
  the dispatch rule sanctions: a landing already costs syscalls, so one `dyn` call beside it is noise.
- **What the drive index knows goes through `OperationIndex`** (`host/index.rs`): a selection's expected totals (the
  scan's progress bar), a folder's recursive size (a conflict prompt's folder sizes), and a subtree's search leaves
  (`journal_search.rs`, which keeps `SEARCH_LEAF_CAP`, the before-the-mutation and after-success rules, and the test
  hook; the index gates and their downgrade reasons are the app's read, `operations_host/index.rs`). Value types
  (`ExpectedTotals`, `BufferedLeaves`, `Leaf`) are the engine's own: it names no `cmdr_index` type.
- **Deliberate slowness for automated runs goes through `Pacing`** (`host/pacing.rs`): the per-step throttle of the copy
  loop (`copy_throttle_for`, which also carries the dialog gallery's per-folder pace), the volume delete batches and the
  archive rewrite's entries (`copy_throttle`), the scan preview's held start, and the rollback runner's
  `pace_between_items`. `None` or non-zero, read per call (a spec changes it mid-run); every real build answers `None`.
  Asked per file in the copy loop: the dispatch rule allows it beside a file's I/O, and the Criterion copy cases showed
  no change against the pre-seam baseline (`docs/notes/transfer-extraction-baseline.md`'s method, 2026-10-17).
- **The collator the panes sort with comes from `NameOrder`** (`host/name_order.rs`), asked once per walk by
  `TreeOrder::active(host)`, which also carries the engine's events, so a tree a bug turned off reports to the engine
  that built it. The engine names `cmdr_collation::NameCollator` directly, never through `cmdr-listing`. Default:
  English, built on first use and kept on the answer itself.
- **Optional seams default to doing nothing.** `EngineHost::new` takes only the runtime's `VolumeHost`, the volumes, and
  the events (nothing works without those); every other seam starts as a no-op answer that `with_*` replaces. The app
  wires them all in `operations_host::app_answers`.
- **The app decides what each fact becomes**: a Tauri event, a `priority::transfers` call, a PostHog capture under a
  literal event name (`analytics-event-catalog` reads `capture("…")`), or an error report. The test-only counters a pin
  reads (`emit_count`, `busy_emit_count`, the scan bridge's tick recorder) count on the engine side, before the seam.
- **The per-op sinks are the app's too.** `TauriEventSink` and `TauriScanPreviewSink` live in
  `apps/desktop/src-tauri/src/operations_host/sinks.rs`, built at the IPC edge and injected; a terminal emit runs its
  app side effects (completion analytics, the MCP ring, the failure `warn!`, `OperationManager::retain_failure`, the
  free-space hint) in the order `apps/desktop/src-tauri/src/operations_host/terminal_effects.rs` pins. An op the ENGINE
  starts on its own behalf (an instant op routed into a zip becomes a managed archive edit) gets its sink from the
  host's `OperationSinkFactory` (`engine.host().operation_sink()`); so does the MCP rollback tool, which is generic over
  the Tauri runtime.
- **Event payloads are plain `serde` + `specta::Type` types**; the Tauri event and its wire name are the app's
  (`apps/desktop/src-tauri/src/operations_host/wire.rs` wraps each). ❗ A payload's doc comments and `#[serde]`
  attributes still reach `bindings.ts`, so they're wire: reword one and `desktop-bindings-fresh` fails.
- **Before `install`, the answer depends on the binary.** In the app, `operations_host::operations()` is
  `Err(NotInstalled)` plus an error report, and each caller refuses with a variant it already handles (a start's
  `IoError`, an instant op's `ArchiveEditNotReady`, `UnknownOperation`) or answers what an engine with nothing running
  would (an empty list, no-op cancels, no busy volume, a scan preview that tells its dialog it couldn't start); nothing
  can reach the engine before `setup()` anyway, and ❌ nothing panics for want of one. The engine itself knows nothing
  about installs. In a test binary, `operations()` builds `operations_host::test_engine()` on first read (the app's own
  answers minus the frontend channel: the real indexing gauge, analytics client, and error reporter, and `cmdr-fs`'s
  shared fallback runtime) and hands that one out to every app-side cell; an engine test builds
  `testing::TestOperations` instead (`crates/cmdr-operations/DETAILS.md` § "Test isolation: an engine of the test's
  own"). No sink factory, so `operation_sink()` stays `None` and an archive-target instant op refuses plainly. The bench
  builds detached engines of its own (`testing::bench::BenchEngine`, `BenchJournal`).
- **A seam trait and its value types live in `host/` or `types/`**, never in a module that calls the seam, or the two
  weld into a cycle. That's why `OperationsChanged` and `OperationSnapshot` are `types` vocabulary, and why
  `OperationEventSink` and `ScanPreviewEventSink` are defined in `host/` (`event_sinks` re-exports them): with the trait
  in `event_sinks`, `state` (which takes a sink) → `event_sinks` → `manager` (the Tauri sink retained failures there) →
  `state` was a cycle `module-cycles` reports.
