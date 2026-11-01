# Synchronize folders (app side)

⇧F12: two folders on any two volumes, compared and made to match. The pure logic is `crates/cmdr-sync`; this module owns
what needs the app. Today that is the saved profiles; sessions and the engine adapter join it with the dialog.

## Module map

- `profile_store.rs`: `sync-profiles.json` (`PROFILES`), the profile list in the person's order plus the options last
  used per pair of roots. List operations (`save_in`, `rename_in`, `move_in`, `delete_in`, `remember_options_in`) are
  pure on `ProfileList`; `ProfileStore` caches, locks, and flushes. IPC: the pass-throughs are `apps/desktop/src-tauri/src/commands/sync.rs`.

## Must-knows

- **Entries are `serde_json::Value`, not `SyncProfile`.** A field a newer build wrote survives this build's save (the
  typed fields are merged into the stored object), and a profile this build can't read is listed as `Invalid` with
  `ProfileProblem::Unreadable`, never dropped. A merge that doesn't read back as exactly the saved profile (a tagged
  value that changed variant) is replaced whole.
- **An invalid profile is shown greyed with its reason**; `save_in` refuses a profile that fails `validate`. Only an
  entry with no string `id` is left out of the list (it can't be addressed), and it stays in the file.
- **A profile stores a volume id and a path, never a credential.** Secrets stay in `secret_store`.
- **Not in `settings.json`**: that file is sparse and per-setting; profiles are an ordered list of records. The store
  follows `multi_rename/preset_store.rs`: temp+rename writes, two locks never held at once, and a file that won't read
  (or has another schema version) is set aside by `state_recovery` and the list starts empty, never saved over.
- **Same name (trimmed, any case) replaces in place**, so no other profile's number moves; a save under an existing id
  renames that profile. The list holds `MAX_PROFILES` (200); the newest `MAX_REMEMBERED_PAIRS` (50) pairs of roots keep
  their last options, the pair being ordered (left, right).
- **The Settings the profiles inherit are frontend-read** (`sync.*` in the registry): the dialog passes them as
  `SyncDefaults` at compare and at run, so no applier case exists. The backend's say on a versions folder is
  `check_sync_keep_policy`.

Schema rules and the decisions: `DETAILS.md`.
