# Operations host

This app as the write engine's host: `install` builds the engine's `EngineHost` in `setup()` and parks it in the
engine's slot (`file_system/write_operations/host/`). `events.rs` is `TauriEngineEvents`, the app's answer to the
engine-wide facts.

## Must-knows

- **`install` runs after `volume_host::install` and before the leftover sweep.** The engine's runtime is
  `volume_host::host()`'s, which is the APP runtime: ❌ never build one from `Handle::current()` in `setup()`, or an
  admitted operation can land on a runtime that dies under it.
- **The engine says WHAT, this module says what the user sees.** Which fact becomes a Tauri event, a
  `priority::transfers` call, or a PostHog capture lives here. An analytics event name stays a LITERAL in its
  `capture("…")` arm, which `analytics-event-catalog` reads.
- **A test binary never installs**, so the engine falls back to `without_frontend()`: these same answers with no app
  handle. The gauge pins in `status_cache_tests` read the real `priority::transfers` through it.

## Module map

- `mod.rs`: `install`, `without_frontend`. `events.rs`: `TauriEngineEvents` and the instant-op analytics props.

Startup order and the fallback's reasoning: `DETAILS.md`.
