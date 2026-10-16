# Operations host

This app as the write engine's host: `install` builds the engine's `EngineHost` in `setup()` and parks it in the
engine's slot (`file_system/write_operations/host/`). Here live the app's answers to every engine seam: the engine-wide
facts, the per-op sinks, and what a terminal event does besides reaching the frontend.

## Must-knows

- **`install` runs after `volume_host::install` and before the leftover sweep.** The engine's runtime is
  `volume_host::host()`'s, which is the APP runtime: ❌ never build one from `Handle::current()` in `setup()`, or an
  admitted operation can land on a runtime that dies under it.
- **The engine says WHAT, this module says what the user sees.** Which fact becomes a Tauri event, a
  `priority::transfers` call, a PostHog capture, or an error report lives here. An analytics event name stays a LITERAL
  in its `capture("…")` arm, which `analytics-event-catalog` reads.
- **A terminal emit's side effects keep their order** (`terminal_effects.rs`): the MCP ring and the retained failure
  land BEFORE the emit. ❌ Never on `OperationEventSink` or a collector sink: test sinks stay side-effect-free.
- **An engine error report keeps its message and target word for word** (`events.rs::raise`), including the engine's
  module path as a literal target where it had none: dispatch filters and log greps key off them.
- **A test binary never installs**, so the engine falls back to `without_frontend()`: these same answers with no app
  handle and no sink factory. The gauge pins in `status_cache_tests` read the real `priority::transfers` through it.

## Module map

- `mod.rs`: `install`, `without_frontend`. `events.rs`: `TauriEngineEvents`, the instant-op analytics props, and the
  error-report re-raise. `sinks.rs`: `TauriEventSink`, `TauriScanPreviewSink`. `terminal_effects.rs` (+ `_tests.rs`):
  the terminal side effects in order. `analytics.rs`: the completion analytics they fire.

Startup order, the sinks, and the fallback's reasoning: `DETAILS.md`.
