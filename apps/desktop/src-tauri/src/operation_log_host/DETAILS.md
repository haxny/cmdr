# Operation log host: details

The app side of the operation log. The log (`../operation_log/`) owns its state in an `OperationLog` handle and knows
nothing about Tauri, settings, or the data dir; this module supplies them.

## Startup

1. `install()` runs in `setup()` right after the volume host, before anything can start an operation, and returns the
   detached handle. A detached handle journals nothing: an operation opened on it stays unjournaled for its whole life.
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

## The engine's journal calls

`journal.rs` keeps the write engine's by-`op_id` shape (`journal_open`, `journal_record_items`, `journal_note_*`,
`journal_finalize`) over `operation_log()`. The handle's by-id methods find each operation's open-time decision again
through its unrecorded set, so an operation stays wholly journaled or wholly not even though the caller drops the
`OpJournal`. Once each operation carries its `OpJournal`, these and the by-id methods go.

- **Decision: these live here, not in `operation_log/`.** Why: they call the app accessor; from inside the log that
  would make `operation_log ↔ operation_log_host` one cycle, and the log could never become its own crate.

## Answers passed in

- **Volumes**: `volumes.rs` implements the planner's `VolumeLookup` for the `VolumeManager`, so a rollback gate and
  each reversed item look volumes up in the real registry, and a test passes a registry (or a fake) of its own.
- **Retention limits**: a closure over `settings::load_operation_log_retention_limits`, read on every tick so a
  settings change applies on the next one.
- **Recording**: seeded here at `start`; live changes arrive through `commands/settings.rs::set_operation_log_enabled`,
  which calls `operation_log().set_recording`.
- **Pacing**: not here. The rollback planner asks its runner (`RollbackRunner::pace_between_items`), and the engine's
  runner answers from `test_mode`.

## Tests

`TestJournalGuard` holds the shared handle's journal slot for one test under a process-wide lock, and resets the slot
and the recording switch on both ends. It exists for the write-pipeline tests, which reach the shared handle by
`op_id`; nextest isolates each test in a process, and the lock covers plain `cargo test`. Its rules (one per test, not
reentrant) are on the type.
