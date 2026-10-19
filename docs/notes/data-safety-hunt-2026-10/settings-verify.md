# Data-safety hunt 2026-10: settings and secrets persistence (verifier report)

Verified against the worktree `data-safety-hunt-2` at `22918588b`, Tauri resolved to 2.12.2 (`Cargo.lock`). All excerpts
are verbatim and current; a few cited line numbers are off by one or two (noted per finding).

### settings-1

- **Verdict:** confirmed
- **Severity:** high (agree). The condition (a present file Cmdr can't read) is unusual but plausible, and the loss is
  silent and unrecoverable.
- **Evidence:**
  - `state_recovery/plugin_stores.rs:54` `recover`: any `fs::read` error other than `NotFound` logs and returns `false`;
    the file is left for the plugin.
  - `vendor/tauri-plugin-store/src/store.rs:234`/`:236` (hunter cited `:232`, the `if !self.create_new` line):
    `let _ = store_inner.load_ignore_defaults()` / `let _ = store_inner.load()`. `load` is `fs::read(&self.path)?` then
    a merge (`store.rs:322`), so a read error leaves the cache at its defaults. `StoreInner` (`store.rs:278`) has no
    dirty flag and no failed-load state; nothing records that the load failed.
  - Exit save is unconditional: `vendor/tauri-plugin-store/src/lib.rs:469` iterates every registered store and calls
    `Store::save` (`store.rs:635`), which only cancels a pending debounce and calls `StoreInner::save`; no change check.
    So a store that was merely LOADED this session is written at quit.
  - The exit save really runs before the resource table is cleared: Tauri 2.12.2 `app.rs:1449` handles
    `RuntimeRunEvent::Exit` by dispatching `RunEvent::Exit` (plugins' `on_event` included) and only then calling
    `cleanup_before_exit` (which clears `resources_table`). Cmdr's quit path ends in `self.app.exit(0)`
    (`quit/mod.rs:471`), which reaches `RunEvent::Exit`; `data_wipe/mod.rs:162` documents the same ordering ("after the
    plugins have saved their stores").
  - `write_atomically` (`store.rs:447`) needs only `fs::metadata` on the target (works without read permission) and a
    writable parent; it renames the fresh file over the unreadable one. For a root-owned `0600` file in the user's own
    data dir (no sticky bit), the rename succeeds.
  - `license.json`: `should_show_commercial_reminder` (`licensing/app_status.rs:361`) is called from every status path
    (`:96`, `:158`, `:257`, `:269`, `:307`); with an empty cache `last_dismissed` is `None`, so it `store.set`s and
    `save_license_file`s immediately, before any quit.
  - No test covers a present-but-unreadable plugin store (`settings/plugin_store_tests.rs` covers permissions kept on
    save and a read-only folder, not an unreadable file).
- **Extra (same root cause, worth fixing together):** `plugin_stores.rs:70`, when `set_aside` of a CORRUPT store fails,
  returns with the comment "The file stays, and the plugin reads it as empty". The plugin's `load` then fails to
  deserialize, the cache is empty, and the exit save overwrites the corrupt file anyway. So the near miss "corrupt
  stores are guarded" holds only when the set-aside rename succeeds. A failed-load flag in the vendored plugin that
  refuses `save` would close both.

### settings-2

- **Verdict:** confirmed
- **Severity:** high (hunter said medium). Same unusual-but-plausible trigger as settings-1, and the favorites list and
  rename presets are user data lost silently and permanently; by the brief's scale that's high. The recents histories
  (search, go-to-path, selection) on the same path are closer to medium.
- **Evidence:**
  - `favorites/store.rs:400`–`415` returns `Some(FavoritesStore::default())` for a non-`NotFound` read error;
    `load_or_seed` (`:453`) caches it in `CACHE` (`:102`) for the session. Nothing ever clears or reloads the cache (no
    `= None` / `take()` on it in the file).
  - `mutate_and_persist` (`:542`) mutates that empty cached store and calls `write_store_to_path` (`:437`) →
    `config::durable_write_json`, which renames over the unreadable original. Any add/rename/reorder/remove-with-effect
    does it.
  - `recents/persistence.rs:32`: `let Ok(contents) = fs::read_to_string(path) else { return Vec::new() }` collapses
    `NotFound`, permission, I/O, and `InvalidData` (non-UTF-8) into "empty", skipping `quarantine_broken`. `load_at`
    (`recents/mod.rs:96`) caches it; `update` (`:163`) writes the whole cached list on the next add/edit (`:177`).
    `multi_rename/presets.rs:17` `PRESETS` is a `RecentsFile`, so saved presets go the same way.
  - Correction to "Guard ruled out": `favorites/store_tests.rs:299` does test the present-but-unreadable read (it pins
    the `Some(empty)`, no-reseed contract), but no test follows it with a mutation, which is where the loss happens.

### settings-3

- **Verdict:** plausible
- **Severity:** medium, low end (agree on medium; the window is narrow and partly self-healing).
- **Evidence:**
  - All windows share one Rust-side store: the plugin's `load` command (`vendor/.../lib.rs:125`) goes through
    `build_inner`, which returns the existing resource id for the same path (`store.rs:213`), so the Settings window's
    `store.get` reads the main window's in-memory cache, which holds X only after main's debounced
    `writeExplicitSettings` runs (`settings-store.ts:578` `scheduleSave`, 500 ms).
  - `initializeSettings` (`settings-store.ts:116`) reads every registry key (`:155` loop) and only afterwards calls
    `setupCrossWindowListener` (`:183`); a `settings:changed` emitted before that isn't replayed. The Settings window is
    created on demand (`settings-window.ts:205`), so its whole boot sits after the toggle.
  - `forceSave` (`settings-store.ts:757`) always writes, and the Settings page calls it on `onDestroy`
    (`+page.svelte:407`) and `beforeunload` (`:425`) with no edit required. `writeExplicitSettings` (`:618`, loop at
    `:631`) `store.delete`s every registry key present in the store but absent from this window's ledger.
  - Close path: `close_child_window` (`commands/child_window_state.rs:36`) hides the window and destroys it 100 ms
    later, so the save's IPCs (one per registry entry) may or may not all land; any `delete` that lands mutates the
    shared Rust cache, which the plugin's exit save then persists (settings-1 mechanics).
- **Narrowing the hunter missed:** the main window's ledger still holds X, and every later `writeExplicitSettings` from
  the main window re-`set`s every explicit key, so ANY subsequent main-window setting change before quit restores X. The
  loss needs: toggle in main, ⌘, before the 500 ms debounce fires AND the Settings webview reaching its read loop before
  it fires, then no further main-window setting save that session. Not reproduced; timing in a release build unmeasured.

### settings-4

- **Verdict:** confirmed
- **Severity:** medium (agree): wedged state, Linux-only, Secret Service absent.
- **Evidence:**
  - `secrets/encrypted_file.rs:37` `encryption_password` (hunter cited `:38`) keys the cocoon on `/etc/machine-id` or a
    fixed fallback.
  - `read_store` (`:46`) returns `Err` on a decrypt (`:54`) or parse (`:66`) failure and leaves the file in place.
  - `set` (`:122`), `delete` (`:143`), and `keys` (`:154`) all call `read_store` first and propagate with `?`, so once
    the key no longer matches, every later save, delete, and listing fails, permanently. Nothing in the file, and no
    `StateFile` in `state_recovery`, ever sets `credentials.enc` aside.
  - No test covers a key mismatch (the module's tests are a roundtrip, serde, and a missing file).
  - Realistic trigger is the home folder carried to a new install or a cloned VM with a regenerated id; on systemd
    `/etc/machine-id` is normally world-readable, so the "sometimes unreadable" variant is rarer.
