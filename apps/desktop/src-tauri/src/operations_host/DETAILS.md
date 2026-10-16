# Operations host: details

The app side of the write engine's seams. The engine declares what it needs in `file_system/write_operations/host/`
(`write_operations/DETAILS.md` § "The engine's host"); this module answers.

## Startup

`install(app)` runs in `setup()` right after `operation_log_host::install()`, which follows `volume_host::install`.
It builds an `EngineHost` from `volume_host::host()` (the engine's runtime), a `TauriEngineEvents` holding the app
handle, and a sink factory that builds a `TauriEventSink`, and hands it to `write_operations::host::install`. That's
before anything can start an operation and before the in-flight sweep, whose staging-leftovers notice goes through it.

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

## What each fact becomes

- `operations_changed` → the `operations-changed` event (a failed emit `warn!`s under `op_manager`).
- `volumes_busy_changed` → `volumes-busy-changed`, wrapped in `VolumesBusyChanged` (a failed emit is a `log_error!`
  under `eject`).
- `transfer_started` / `transfer_finished` → `priority::transfers::note_transfer_started` / `_finished`.
- `staging_leftovers_kept` → `move-leftovers-kept` (a failed emit `warn!`s under `copy`).
- `instant_mutation_settled` → `rename_used` / `folder_created` / `file_created`, with the `initiator`, `target`, and
  `outcome` tokens `analytics/DETAILS.md` § "Starter event set" catalogs.
- `error_report` → `log_error!`, one arm per `OperationErrorReport` variant, each with the message and target the
  engine's code always raised it with. A report that had no explicit target keeps the engine module's path as a
  literal (`cmdr_lib::file_system::write_operations::overwrite`, …), so `RUST_LOG` recipes and the report dispatch's
  per-chain filters keep matching wherever the engine's code lives.
- `scan_wait_progress` → `write-progress` (a failed emit `warn!`s under `op_manager`).

## A test binary

Nothing installs in a test binary, and some engine pins read the app's real answers: the transfer-gauge cells in
`status_cache_tests` assert on `priority::transfers`. So before `install` the engine's slot answers, under `cfg(test)`
only, with `without_frontend()`: `TauriEngineEvents` with no app handle (events go nowhere; the gauge, the analytics
client, and the error reporter work as in the app) over `volume_host::host()`, whose runtime is `cmdr-fs`'s shared
fallback, and no sink factory. Outside tests the pre-install answer is a detached host that drops everything; nothing
can start an operation before setup.
