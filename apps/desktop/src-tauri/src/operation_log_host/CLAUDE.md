# Operation log host

This app as the operation log's host: `install` builds the app's one `OperationLog` at the top of `setup()`,
`operation_log()` hands it out, and `start` opens it once the data dir and settings are known. The log itself is
`crates/cmdr-operation-log/CLAUDE.md`.

## Must-knows

- **`install` runs early and `start` opens the SAME handle**, passed by value from `install`'s return: ❌ never a
  second `operation_log()` read in `start`, which could pick up an early detached handle.
- **A read before `install` is loud** (`log_error!`, a separate detached handle), so the slot stays free. Only the test
  and bench binaries build their handle on first read.
- **`journal.rs` holds the write engine's by-`op_id` journal calls**, over the app accessor the crate can't name.
- **An app answer goes in as a value or a trait** (`VolumeLookup` over the engine's volumes, the retention-limits
  closure over settings, the app's runtime): `cmdr-operation-log` has no `tauri` and never names settings or this module.
- **A write-pipeline test that wants journaling holds `TestJournalGuard`** (one per test); a test of the log itself
  builds its own `OperationLog` and needs none. The guard reaches the crate's `testing` gate (`set_journal`,
  `reset_recording`), which the app's dev-dependency turns on.

## Module map

- `mod.rs`: `install`, `operation_log()`, `start`, the retention limits. `journal.rs`: the engine's by-id calls.
  `volumes.rs` (test-only): `VolumeLookup` for a `VolumeManager`. `test_journal.rs`: `TestJournalGuard`.

Startup order and why each answer is passed rather than reached: `DETAILS.md`.
