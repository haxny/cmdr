# Operations host: details

The app side of the write engine's seams. The engine declares what it needs in `file_system/write_operations/host/`
(`write_operations/DETAILS.md` § "The engine's host"); this module answers.

## Startup

`install(app)` runs in `setup()` right after `operation_log_host::install()`, which follows `volume_host::install`.
It builds an `EngineHost` from `volume_host::host()` (the engine's runtime) and a `TauriEngineEvents` holding the app
handle, and hands it to `write_operations::host::install`. That's before anything can start an operation and before
the in-flight sweep, whose staging-leftovers notice goes through it.

- **Decision: the runtime comes from the volume host, with no second handle.** Why: the engine and the backends it
  drives then share one pool, and the two can't disagree about which runtime is the app's. The admission pass
  depends on it being the app's (`write_operations/DETAILS.md` § "The admission pass spawns admitted ops on the APP
  runtime").
- **Decision: the `EngineHost` also carries the app handle for now**, for the two engine paths that still build a
  Tauri sink themselves (the archive-edit instant routes and the scan bridge's progress forward). It leaves when those
  sinks move app-side.

## What each fact becomes

- `operations_changed` → the `operations-changed` event (a failed emit `warn!`s under `op_manager`).
- `volumes_busy_changed` → `volumes-busy-changed`, wrapped in `VolumesBusyChanged` (a failed emit is a `log_error!`
  under `eject`).
- `transfer_started` / `transfer_finished` → `priority::transfers::note_transfer_started` / `_finished`.
- `staging_leftovers_kept` → `move-leftovers-kept` (a failed emit `warn!`s under `copy`).
- `instant_mutation_settled` → `rename_used` / `folder_created` / `file_created`, with the `initiator`, `target`, and
  `outcome` tokens `analytics/DETAILS.md` § "Starter event set" catalogs.

## A test binary

Nothing installs in a test binary, and some engine pins read the app's real answers: the transfer-gauge cells in
`status_cache_tests` assert on `priority::transfers`. So before `install` the engine's slot answers, under `cfg(test)`
only, with `without_frontend()`: `TauriEngineEvents` with no app handle (events go nowhere, the gauge and the
analytics client work as in the app) over `volume_host::host()`, whose runtime is `cmdr-fs`'s shared fallback. Outside
tests the pre-install answer is a detached host that drops everything; nothing can start an operation before setup.
