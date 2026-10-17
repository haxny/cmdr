# Operation log host

This app as the operation log's host: `install` builds the app's one `OperationLog` at the top of `setup()`,
`operation_log()` hands it out, and `start` opens it once the data dir and settings are known. The log itself is
`crates/cmdr-operation-log/CLAUDE.md`.

## Must-knows

- **`install` runs early and `start` opens the SAME handle**, passed by value from `install`'s return: ❌ never a
  second `operation_log()` read in `start`, which could pick up an early detached handle.
- **A read before `install` is loud** (`log_error!`, a separate detached handle), so the slot stays free. Only the test
  binary builds its handle on first read.
- **The write engine journals into the handle `install` returned**, passed to `operations_host::install`, ❌ never a
  read of `operation_log()` from engine code.
- **An app answer goes in as a value or a trait** (`VolumeLookup` over the engine's volumes, the retention-limits
  closure over settings, the app's runtime): `cmdr-operation-log` has no `tauri` and never names settings or this module.
- **A test that wants journaling builds its own log** (`write_operations::testing::TestOperationLog`) and runs its
  operations on an engine journaling into it (app-side: `operations_host::test_engine_journaling_into`), ❌ never
  on the shared `operation_log()`. A test of the log itself builds an `OperationLog`.

## Module map

- `mod.rs`: `install`, `operation_log()`, `start`, the retention limits, the unreadable-DB set-aside.
  `volumes.rs` (test-only): `VolumeLookup` for a `VolumeManager`.

Startup order and why each answer is passed rather than reached: `DETAILS.md`.
