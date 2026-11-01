# Synchronize folders (app side)

⇧F12: two folders on any two volumes, compared and made to match. The pure logic is `crates/cmdr-sync`; this module owns
what needs the app: saved profiles, the dialog's compare and run sessions, and the `SyncEngine` adapter. IPC
pass-throughs: `apps/desktop/src-tauri/src/commands/sync.rs`.

## Module map

- `profile_store.rs`: `sync-profiles.json`, the profiles in the person's order plus last options per pair of roots.
- `session.rs` (registry, rows, overrides), `session_compare.rs`, `session_run.rs` (confirm, run), `run_lock.rs`: one
  session per dialog over `host.rs`'s `SyncHost` seam. `wire.rs`: IPC shapes and the five events.
- `real_paths.rs`: real paths off the session lock (roots at compare and run, custom versions folders).
- `engine.rs` + `engine_ops.rs`: `SyncEngine` over `cmdr-operations`; `settle_sink.rs` wraps the app's sink per op.

## Must-knows

- **Profile entries are raw JSON**, merged with the typed fields on save, so a newer build's fields survive; an
  unreadable one is listed `Invalid`, never dropped. No credentials, ever: a volume id and a path.
- **Settings are frontend-read** and arrive as `SyncDefaults`; every compare, confirm, and run checks them
  (`validate_defaults` on Settings plus overrides).
- **Roots in this Mac's namespace compare by their real path** (symlinks resolved), whatever volume id named them: for
  the same-or-nested check, the run lock, and `InsideSyncFolder`.
- **A run is tied to the confirm the person saw**: `RunRequest::revision` and its Settings must match the plan's
  current revision, which every override, content answer, and new Settings bump. The run then calls `gate` itself.
- **Only the newest compare of a session speaks**: a superseded one emits nothing; events carry the generation.
- **The adapter never deletes**: landings are `ConflictResolution::Skip`, claims exclusive, empty-folder removal one
  `rmdir`, removals go to the Trash or a versions folder; no symlink inside a root may lead a write.
- **`SettleSink` wraps the app's sink, never replaces it**: the queue window and the engine's questions must still
  reach the person.

Schema rules, sessions, and the adapter's decisions: `DETAILS.md`.
