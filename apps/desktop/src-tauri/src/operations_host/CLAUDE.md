# Operations host

This app as the write engine's host: `install` builds the app's one `Operations` in `setup()` and `operations()` hands
it out. Here live the app's answers to every engine seam (`file_system/write_operations/host/`): the volumes, the
engine-wide facts, the per-op sinks, and what a terminal event does besides reaching the frontend.

## Must-knows

- **`install` runs after `volume_host::install` and `operation_log_host::install` (whose handle it takes), before the
  leftover sweep, unconditionally.** The engine's runtime is `volume_host::host()`'s, which is the APP runtime: ❌ never
  build one from `Handle::current()` in `setup()`, or an admitted operation can land on a runtime that dies under it.
- **Before `install`, `operations()` is `Err(NotInstalled)`**: a start refuses through `refuse_not_installed`, ❌ never
  runs on a detached engine.
- **The engine says WHAT, this module says what the user sees.** Which fact becomes a Tauri event, a
  `priority::transfers` call, a PostHog capture, or an error report lives here. An analytics event name stays a LITERAL
  in its `capture("…")` arm, which `analytics-event-catalog` reads.
- **An engine payload goes out wrapped**: `wire::WriteProgress(event).emit(&app)` (`wire.rs`), one `wire_event!` line
  per payload, registered in `ipc.rs`. ❌ Never derive the wrapper's `specta::Type`: the hand-written one is what keeps
  `bindings.ts` free of an alias.
- **A terminal emit's side effects keep their order** (`terminal_effects.rs`): the MCP ring and the retained failure
  land BEFORE the emit. ❌ Never on `OperationEventSink` or a collector sink: test sinks stay side-effect-free.
- **An engine error report keeps its message and target word for word** (`events.rs::raise`), including the engine's
  module path as a literal target where it had none: dispatch filters and log greps key off them.
- **A test binary never installs**, so its first read builds `test_engine()`: these same answers with no app handle and
  no sink factory. The gauge pins in `status_cache_tests` read the real `priority::transfers` through it.

## Module map

- `mod.rs`: `install`, `operations()`, `refuse_not_installed`, `test_engine`. `events.rs`: `TauriEngineEvents`, the instant-op analytics props, and the
  error-report re-raise. `sinks.rs`: `TauriEventSink`, `TauriScanPreviewSink`. `terminal_effects.rs` (+ `_tests.rs`):
  the terminal side effects in order. `analytics.rs`: the completion analytics they fire. `wire.rs`: the payloads as
  Tauri events. `volumes.rs`: `AppOperationVolumes`, the volume registry asked per call. `own_writes.rs`: `AppOwnWrites`, the downloads
  watcher's ignore set. `index.rs`: `AppOperationIndex`, the drive index's counts and search leaves. `pacing.rs`: `AppPacing`, `test_mode`'s
  E2E throttles. `name_order.rs`: `AppNameOrder`, the panes' collator.

Startup order, the sinks, the wire, and the fallback's reasoning: `DETAILS.md`.
