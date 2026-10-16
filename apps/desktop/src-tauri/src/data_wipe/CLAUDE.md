# Data wipe

"Delete all Cmdr data" (Settings › Updates & privacy): removes everything Cmdr keeps on this Mac, then the frontend
quits the app. Meets the CRA's Annex I Part I (2)(m). Commands: `../commands/data_wipe.rs`. Dialog and flow:
`apps/desktop/src/lib/settings/sections/DeleteAllDataDialog.svelte`.

## Module map

- `plan.rs`: the targets, from this instance's own roots (`WipeRoots`), plus `is_safe_target`. Pure.
- `execute.rs`: secrets, then each target through a tombstone rename, then `WipeReport`'s leftovers. The
  tombstone sweep and the exit pass.
- `summary.rs`: the sizes the dialog quotes. `mod.rs`: roots, quiesce, `has_wiped`, `on_exit`, `OsHost`.

## Must-knows

- **Every root comes from the app's own path helpers** (`config::resolved_app_data_dir`, `config::app_cache_dir`,
  `logging::log_dir`, the Tauri identifier), ❌ never a hardcoded bundle id: a dev, worktree, or E2E instance wipes only
  its own folders, and its Keychain items only (the service name is instance-suffixed).
- **`is_safe_target` is the last line against a misresolved root** (home, `~/Library`, a folder directly in either, or
  a `~/Library` entry without the identifier in its name). ❌ Never loosen it to make a target fit.
- **Whole targets are renamed to `<name>.cmdr-wipe-<random>` first, then removed**, so a crash leaves either the
  original or a tombstone nothing reads; `sweep_interrupted` (launch) finishes it. ❌ Never delete in place first.
- **Secrets go through `SecretStore::keys` + `delete`**, so the file store (dev, E2E) is covered too. The leftover count
  is a re-list at the END, so a file store's items gone with the data dir don't read as leftovers.
- **Symlinks are removed as links, never followed** (`remove_any`): one pointing at the person's folder must not take it.
- **After a wipe the app must quit, and exit writers check `has_wiped()`.** The store plugin saves every store on
  `RunEvent::Exit` before our handler runs, so `on_exit` removes the targets once more, last.
- **Refuses while a file operation runs** (`FileOperationsRunning`, the quit gate's `blocks_quit`); no deadline on the
  wipe itself, since a "timed out" while deletion goes on would lie.

Decisions, what's quiesced and why not the rest, the residuals: `DETAILS.md`.
