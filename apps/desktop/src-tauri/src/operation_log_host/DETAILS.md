# Operation log host: details

The app side of the operation log. The log (`crates/cmdr-operation-log/`) owns its state in an `OperationLog` handle and knows
nothing about Tauri, settings, or the data dir; this module supplies them.

## Startup

1. `install()` runs in `setup()` right after the volume host, before the write engine is built
   (`operations_host::install` takes the handle), and returns the detached handle. A detached handle journals nothing: an operation opened on it stays unjournaled for its whole life.
2. `start(app, &log)` runs late (beside `agent::start`), once the data dir resolves: it seeds the recording switch from
   `settings.json`, calls `OperationLog::open` (store, writer, reconcile, then publish), and starts retention on the
   app's runtime. A data-dir or open failure logs a warning and leaves the app running without history, as before the
   handle existed.

- **Decision: `start` takes the handle `install` returned.** Why: a re-read through `operation_log()` could, in a
  mis-ordered startup, open a detached placeholder while the engine journals into the installed one. Passing it makes
  "the handle that opens is the handle everyone reads" true by construction.
- **Decision: a pre-install read answers from a separate detached handle and is reported**, the listing host's
  pattern (`../listing_host/DETAILS.md`). Why: claiming the slot with an early placeholder would make `install` lose
  silently, and every later journal call would go nowhere. The test and bench binaries never call `install`, so there
  the first read builds the shared handle.

## The write engine's journal

`install` returns the handle, and `operations_host::install` hands that same handle to the write engine, which journals
into it (`Operations::operation_log`) and never reads `operation_log()`. The test binary's shared engine is built over
the shared handle too, so `TestJournalGuard` reaches what the write pipeline journals into.

## Answers passed in

- **Volumes**: the planner's `VolumeLookup` is the write engine's `OperationVolumes` (its supertrait), so a rollback
  gate and each reversed item look volumes up through the engine's host, which is the real registry in the app.
  `volumes.rs` implements it for a bare `VolumeManager`, test-only, so a planner cell passes a registry of its own.
- **Retention limits**: a closure over `settings::load_operation_log_retention_limits`, read on every tick so a
  settings change applies on the next one.
- **Recording**: seeded here at `start`; live changes arrive through `commands/settings.rs::set_operation_log_enabled`,
  which calls `operation_log().set_recording`.
- **Pacing**: not here. The rollback planner asks its runner (`RollbackRunner::pace_between_items`), and the engine's
  runner answers from its host's `Pacing`, which `operations_host/pacing.rs` answers from `test_mode`.

## Tests

`TestJournalGuard` holds the shared handle's journal slot for one test under a process-wide lock, and resets the slot
and the recording switch on both ends. It exists for the write-pipeline tests, which reach the shared handle by
`op_id`; nextest isolates each test in a process, and the lock covers plain `cargo test`. Its rules (one per test, not
reentrant) are on the type.
