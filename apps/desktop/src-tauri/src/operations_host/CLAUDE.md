# Operations host

This app as the write engine's host: `install` builds the app's one `Operations` in `setup()` and `operations()` hands
it out. Here live the app's answers to every engine seam (`crates/cmdr-operations/src/host/CLAUDE.md`): the volumes,
the engine-wide facts, the per-op sinks, and what a terminal event does besides reaching the frontend.

## Must-knows

- **`install` runs after `volume_host::install` and `operation_log_host::install` (whose handle it takes), before the
  ledger claim, unconditionally.** `claim_ledger` claims the installed engine's ledger, after the instance lock; ❌
  never another engine's. The engine's runtime is `volume_host::host()`'s, the APP runtime: ❌ never one built from
  `Handle::current()` in `setup()`, or an admitted operation lands on a runtime that dies under it.
- **Before `install`, `operations()` is `Err(NotInstalled)`**: a caller refuses through a `refuse_*_not_installed`
  answer, ❌ never runs on a detached engine.
- **The engine says WHAT, this module says what the user sees**: a Tauri event, a `priority::transfers` call, a PostHog
  capture, or an error report. An analytics event name stays a LITERAL in its `capture("…")` arm
  (`analytics-event-catalog` reads it).
- **An engine payload goes out wrapped**: `wire::WriteProgress(event).emit(&app)`, one `wire_event!` line per payload,
  registered in `ipc.rs`. ❌ Never derive the wrapper's `specta::Type`: the hand-written one keeps `bindings.ts` free of
  an alias.
- **A terminal emit's side effects keep their order** (`terminal_effects.rs`): the MCP ring and the retained failure
  land BEFORE the emit. ❌ Never on `OperationEventSink` or a collector sink: test sinks stay side-effect-free.
- **An engine error report keeps its message and target word for word** (`events.rs::raise`): dispatch filters and log
  greps key off them.
- **A test binary never installs**: its first read builds `test_engine()` (these answers, no app handle, no sink
  factory), which every app-side cell shares (`app_engine()`) with unique ids. An engine suite builds
  `testing::TestOperations` instead.
- **Cells here reach the engine through its public list plus `cmdr_operations::testing`**, ❌ never an engine module by
  path.

## Module map

- `mod.rs`: `install`, `claim_ledger`, `operations()`, the `refuse_*_not_installed` answers, `test_engine` (+
  `_journaling_into`), `app_engine`. `events.rs` (`TauriEngineEvents`), `sinks.rs` (the two Tauri sinks),
  `terminal_effects.rs` + `analytics.rs` (terminal side effects), `wire.rs` (payloads as Tauri events).
- One adapter per seam: `volumes.rs`, `own_writes.rs`, `index.rs`, `pacing.rs`, `name_order.rs`. `backend_suites/`:
  every backend's cells through the app's engine.

Startup order, the sinks and why their side effects live here, the wire, the test binary, and the backend suites:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
