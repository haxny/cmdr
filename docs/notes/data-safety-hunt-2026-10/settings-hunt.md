# Data-safety hunt 2026-10: settings and secrets persistence (hunter report)

Slice: `secrets/`, `settings/`, `recents/`, `favorites/`, `window_state/`, `child_window_state.rs`, `install_id.rs`,
`licensing/` (stored state), `agent/memory/`, `state_recovery/`, `data_wipe/`, `quit/`, `vendor/tauri-plugin-store`, and
the frontend settings store (`apps/desktop/src/lib/settings/`).

Four findings survived. The common thread of the top two: the corrupt-file path is well guarded (set aside, notice,
fresh start), but a file that is PRESENT and fails to READ for an I/O reason is deliberately "left alone", and then
overwritten anyway by the next save.

### settings-1. An unreadable `settings.json`, `shortcuts.json`, `app-status.json`, or `license.json` is replaced with an empty store on the next quit

- **Severity:** high
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/state_recovery/plugin_stores.rs:56`, `recover`;
  `vendor/tauri-plugin-store/src/store.rs:232`, `StoreBuilder::build_inner`; `vendor/tauri-plugin-store/src/lib.rs:469`
  (the `RunEvent::Exit` save of every loaded store); `vendor/tauri-plugin-store/src/store.rs:447`, `write_atomically`;
  `apps/desktop/src-tauri/src/licensing/app_status.rs:361`, `should_show_commercial_reminder` (an automatic
  `license.json` write on the first status check)
- **Excerpt:**

```rust
// apps/desktop/src-tauri/src/state_recovery/plugin_stores.rs:54
fn recover(path: &Path, file: StateFile) -> bool {
    sweep_killed_saves(path);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        Err(e) => {
            // Present but unreadable right now: we can't tell it's damaged, so it stays put.
            log::warn!(target: LOG_TARGET, "Couldn't read {path:?} to check it, leaving it as is: {e}");
            return false;
        }
    };
```

```rust
// vendor/tauri-plugin-store/src/store.rs:232
        if !self.create_new {
            if self.override_defaults {
                let _ = store_inner.load_ignore_defaults();
            } else {
                let _ = store_inner.load();
            }
        }
```

```rust
// vendor/tauri-plugin-store/src/lib.rs:469
                if let RunEvent::Exit = event {
                    let collection = app_handle.state::<StoreState>();
                    let stores = collection.stores.read().unwrap();
                    for (path, rid) in stores.iter() {
                        if let Ok(store) = app_handle.resources_table().get::<Store<R>>(*rid)
                            && let Err(err) = store.save()
```

- **Mechanism:**
  1. `setup()` runs `recover_plugin_stores` (`lib.rs:332`). For a file whose `fs::read` fails with anything but
     `NotFound` (EACCES, EIO), `recover` logs and returns `false`, leaving the file in place on purpose.
  2. The frontend's `load('settings.json', { defaults: {} })` (and `shortcuts.json`, `app-status.json`), and Rust's
     `app.store("license.json")`, reach `build_inner`, whose `load()` does `fs::read(...)?` and fails; the error is
     discarded with `let _ =`, so the store's cache is empty (or just its defaults).
  3. Nothing marks the store as "failed to load". On quit, the plugin's `RunEvent::Exit` handler calls `save()` on every
     loaded store unconditionally, with no change required.
  4. `write_atomically` needs only `stat` on the target (`fs::metadata`, which works without read permission) and write
     permission on the parent dir: it creates a fresh temp file, writes `{}` (or the few keys set this session), and
     renames it over the unreadable original.
  5. For `license.json` it doesn't even wait for quit: the first `get_app_status` reads no key from the empty store,
     returns `Personal`, and `should_show_commercial_reminder` sets `commercial_reminder_last_dismissed` and calls
     `save_license_file` right away.
- **Trigger:** the file is readable by nobody but someone else (left `0600` and owned by root after Cmdr was once run
  with `sudo`, a restore or migration tool that reset ownership or mode, an ACL), or a disk read error on that sector.
  The person then launches Cmdr and quits it. No other action is needed.
- **Guard ruled out:** `recover` intends to protect the file but only stops itself from touching it; the plugin's load
  and exit save have no failed-load state. The patched `write_atomically` makes the write atomic, which doesn't help
  here (it atomically installs the empty store). `settings-store.ts`'s sparse ledger only limits what it writes during
  the session; the exit save is the plugin's own. `plugin_store_tests.rs` and the `plugin_stores.rs` tests cover corrupt
  and missing files, never a present-but-unreadable one. No `state_recovery` notice fires, so the person isn't told.
- **Impact:** every setting (including the consent "no"s, which return to their default ON, the very thing
  `CONSENT_OFF_AFTER_RECOVERY` exists to prevent), every custom shortcut, the saved tabs and layout (`app-status.json`),
  and the `license_key_in_secret_store` marker, so a paid license reads as Personal until the key is entered again (the
  key itself stays orphaned in the Keychain). The original file is gone; not recoverable short of a backup.
- **Suggested fix:** in `recover`, set an unreadable-but-present file aside too (or copy it to a `.broken-` sibling and
  refuse to load the store), or teach the patched plugin to remember a failed `load` and refuse every `save` of that
  store for the session (`Error::Unloaded`), which also covers the exit save.

### settings-2. An unreadable `favorites.json` or multi-rename presets file is overwritten by the next edit

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/favorites/store.rs:400`, `read_store_from_path`;
  `apps/desktop/src-tauri/src/favorites/store.rs:542`, `mutate_and_persist`;
  `apps/desktop/src-tauri/src/recents/persistence.rs:32`, `read` (used by `multi_rename/presets.rs:17` `PRESETS`, and
  the search, selection, go-to-path, and field histories)
- **Excerpt:**

```rust
// apps/desktop/src-tauri/src/favorites/store.rs:400
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            // The file is PRESENT but unreadable right now (transient I/O error, permission blip).
            // Returning `None` here would signal "first launch" and re-seed the defaults OVER the
            // user's real list (data loss). Instead read as an empty store WITHOUT overwriting disk:
            // `load_or_seed` only writes when the read is `None`, so the unreadable file is left
            // intact for a later successful read. We don't quarantine, since we couldn't read it to
            // confirm it's actually corrupt.
            log::warn!(
                target: "favorites::store",
                "Couldn't read favorites at {path:?} ({e}); treating as present (no re-seed) to protect the user's list"
            );
            return Some(FavoritesStore::default());
        }
    };
```

```rust
// apps/desktop/src-tauri/src/recents/persistence.rs:29
pub(super) fn read<E: RecentEntry>(path: &Path) -> Vec<E> {
    cleanup_tmp_file(path);

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
```

- **Mechanism:**
  1. `load_or_seed` gets `Some(FavoritesStore::default())` for an unreadable file and caches it in `CACHE` for the whole
     session; there's no "later successful read", because the cache is never reloaded.
  2. The person sees an empty favorites menu and adds one (or reorders, which always writes). `mutate_and_persist`
     mutates the empty cached store and calls `write_store_to_path`, whose `durable_write_json` renames the one-entry
     file over the unreadable original.
  3. Recents is the same shape with less ceremony: any read error, including non-UTF-8 bytes (`read_to_string`'s
     `InvalidData`), returns an empty list without a set-aside, and the next `add`/`edit` (`RecentsFile::update`)
     rewrites the file. For `multi-rename-presets.json` that is the person's saved presets.
- **Trigger:** the same unreadable-file condition as settings-1, then one ordinary gesture: adding a favorite, saving a
  rename preset. For recents, also a file with a corrupt non-UTF-8 byte.
- **Guard ruled out:** the comment's "left intact for a later successful read" holds only until the first mutation;
  `claim_volumes` doesn't write on an empty list (`claim_in_store` returns `false`), but every user gesture does. The
  quarantine path (`quarantine_broken`) runs only for parse and schema failures. `store_tests.rs` and `persistence.rs`'s
  tests cover missing and corrupt files, not unreadable ones.
- **Impact:** the whole favorites list (paths, labels, letter shortcuts) or every saved rename preset, gone with no
  notice and no copy. Not recoverable.
- **Suggested fix:** keep an "unreadable" flag beside the cache and refuse writes while it's set (or set the file aside
  through `state_recovery::set_aside` before starting empty, as the parse-failure path does); give `recents::read` the
  same three-way match favorites has, with the non-UTF-8 case going to `quarantine_broken`.

### settings-3. A setting changed just before Settings opens is deleted from `settings.json` when Settings closes

- **Severity:** medium
- **Confidence:** plausible (the timing window is real in code; how often it's hit in a release build isn't measured)
- **Where:** `apps/desktop/src/lib/settings/settings-store.ts:618`, `writeExplicitSettings`;
  `apps/desktop/src/lib/settings/settings-store.ts:155` (the ledger built at load) and `:284` (the cross-window listener
  set up only after it); `apps/desktop/src/routes/settings/+page.svelte:407` (the close-time `forceSave`)
- **Excerpt:**

```ts
// apps/desktop/src/lib/settings/settings-store.ts:618
async function writeExplicitSettings(): Promise<void> {
  const store = await getStore()
  const existingKeys = new Set(await store.keys())

  // Sparse write: persist exactly the explicitly-set keys (structural — driven
  // by which mutator ran, NEVER a value compare against the default), and drop
  // any registry key that's persisted but no longer explicit (e.g. after a
  // reset) so it resolves back to the registry default. Non-registry keys are
  // left untouched: this loop only knows registered ids, so a key that left the
  // registry needs a `migrateSettings()` case that deletes it.
  let savedCount = 0
  let removedCount = 0

  for (const def of settingsRegistry) {
    const id = def.id
    if (explicitlySet.has(id)) {
      await store.set(id, settingsCache.get(id))
      savedCount++
    } else if (existingKeys.has(id)) {
      await store.delete(id)
      removedCount++
    }
  }
```

- **Mechanism:**
  1. In the main window, `setSetting(X)` updates its own cache and ledger, emits `settings:changed`, and schedules the
     store write 500 ms out (`SAVE_DEBOUNCE_MS`). The shared Rust store doesn't hold X yet.
  2. The Settings window is created on demand (`settings-window.ts:205`) and runs `initializeSettings` inside that 500
     ms: it reads every key from the shared store (no X, so X is absent from ITS ledger), and only then subscribes to
     `settings:changed` (`setupCrossWindowListener`), so the event for X was emitted before anyone in that window
     listened.
  3. The main window's debounced save lands and the store now holds X.
  4. When the Settings window closes, `onDestroy` calls `forceSave()` unconditionally, even with no change made there.
     `writeExplicitSettings` sees X in `existingKeys`, not in its ledger, and `store.delete(X)`s it, then saves.
  5. The main window gets no event for the deletion (it doesn't listen to `store://change`), so it keeps running on X
     for the session; the next launch reads X at its default.
- **Trigger:** toggle a setting from the main window (show hidden files from the menu, a view option, a toast that sets
  something) and open Settings (⌘,) before the 500 ms debounce fires; later close Settings. Any other full-store window
  opened in that window (Debug, the onboarding flows that call `forceSave`) has the same exposure.
- **Guard ruled out:** the cross-window event is the only sync and it isn't replayed to a late subscriber; the
  `explicit` flag mirrors sets and resets but can't cover a set that predates the listener. The settings-store
  persistence tests exercise the ledger between windows that both exist, not a window born mid-debounce.
- **Impact:** one setting silently reverts on the next launch (wrong result; the person can set it again). A consent
  "no" given from a toast could revert to the default "yes" the same way.
- **Suggested fix:** have a newly initialized window never delete keys it didn't see at load: track "keys this window
  reset" separately and delete only those, or have `writeExplicitSettings` delete only ids this window explicitly reset
  rather than every non-ledger key present on disk.

### settings-4. On Linux without a keyring, a changed machine id wedges the credential file so no password can be saved again

- **Severity:** medium
- **Confidence:** confirmed (traced end to end)
- **Where:** `apps/desktop/src-tauri/src/secrets/encrypted_file.rs:38`, `encryption_password`;
  `apps/desktop/src-tauri/src/secrets/encrypted_file.rs:46`, `read_store`; `:122`, `EncryptedFileStore::set`
- **Excerpt:**

```rust
// apps/desktop/src-tauri/src/secrets/encrypted_file.rs:38
fn encryption_password() -> Vec<u8> {
    std::fs::read_to_string("/etc/machine-id")
        .unwrap_or_else(|_| "cmdr-credential-store".to_string())
        .trim()
        .as_bytes()
        .to_vec()
}

fn read_store(path: &PathBuf) -> Result<StoreContents, SecretStoreError> {
    if !path.exists() {
        return Ok(StoreContents::default());
    }
    let encrypted = std::fs::read(path)
        .map_err(|e| SecretStoreError::Other(format!("Could not read encrypted credential file: {}", e)))?;
    let password = encryption_password();
    let cocoon = Cocoon::new(&password);
    let decrypted = cocoon.unwrap(&encrypted).map_err(|e| {
```

- **Mechanism:**
  1. `credentials.enc` is encrypted with a key from `/etc/machine-id`, or the fixed fallback string when that file can't
     be read at write time.
  2. The machine id changes (the home folder restored onto a reinstalled or new machine, a cloned VM that regenerated
     its id), or the write and the read disagree on whether `/etc/machine-id` was readable. `cocoon.unwrap` fails.
  3. `read_store` returns an error, which is right for `get`. But `set` and `delete` call `read_store` first and bail,
     so every later save fails too. Nothing sets the file aside (`state_recovery` doesn't know it), so the store stays
     wedged forever: each SMB, SFTP, WebDAV, or AI-key save logs a warning and the person is asked for the password on
     every connection.
- **Trigger:** a Linux user with no Secret Service (the `EncryptedFileStore` fallback) moves their home folder to a new
  install, or runs in an environment where `/etc/machine-id` is sometimes unreadable.
- **Guard ruled out:** the "leave in place" branch was added to stop a corrupt file being overwritten (correct), but
  with no recovery path it trades silent loss for a permanent wedge. `is_file_backed` only drives a one-time info toast.
  No test covers a key mismatch.
- **Impact:** the old credentials are unrecoverable either way (wrong key); the new harm is that the person can never
  store a credential again until they find and delete `credentials.enc` by hand. Wedged state.
- **Suggested fix:** on a decrypt or parse failure in `set`, hand the file to `state_recovery::set_aside` (a new
  `StateFile::Credentials`) and start a fresh store, so the person loses what was already lost and is told once.

## Near misses

- Corrupt (unparseable) plugin stores: `recover_plugin_stores` sets them aside before any load, and the consent settings
  restart OFF. Guarded.
- Truncated store save on crash or full disk: the vendored plugin's `write_atomically` (temp, fsync, rename, dir fsync),
  pinned by `settings/plugin_store_tests.rs`. Guarded.
- `PlainFileStore` / `EncryptedFileStore` corrupt file replaced by an empty one: both now error and leave it in place,
  with a regression test (`test_read_store_corrupt_file_returns_error_not_default`). Guarded (see settings-4 for the
  wedge it leaves on Linux).
- A release build downgraded to plaintext secrets by `CMDR_SECRET_STORE=file`: `PLAINTEXT_OVERRIDE_HONORED` is false in
  release, tested. Guarded.
- Secrets in logs: Keychain/keyring backends log only the key name at debug (a server or provider id, never the value);
  `data_wipe` keeps even the key out. `secrets.json` and `credentials.enc` are created `0600` with the mode baked in.
- `data_wipe` reaching outside Cmdr's dirs: `is_safe_target` refuses home, `~/Library`, and their direct children, and
  any `~/Library` entry without the identifier; `remove_any` never follows symlinks; tombstone rename first. A user who
  points `CMDR_DATA_DIR` (honored in release) at an existing folder of their own would have that folder wiped, but
  that's the folder they told Cmdr to own.
- Dev/worktree instance writing prod: the wrapper gives every instance its own identifier and `CMDR_DATA_DIR`,
  `resolveStorePath` routes the frontend stores there, the Keychain service name is instance-suffixed, and the wipe
  plans from the instance's own roots. `license.json` follows the identifier, not `CMDR_DATA_DIR`, so an E2E run's lands
  in `~/Library/Application Support/<e2e identifier>/`, never prod's.
- Favorites first-launch re-seed over a real list: the absent-versus-present `Option` contract, tested. Guarded.
- License migration losing the key: the file loses the plaintext key only after a verified secret-store read-back,
  tested. Guarded.
- License key unreadable (Keychain denied) driving a destructive write: `KEY_UNREADABLE` only reads as Personal for the
  session; nothing clears the marker or the key.
- Agent memory jail escapes (`..`, absolute, symlinked components, symlinked parent): lexical check plus symlink walk
  plus canonical-parent containment, tested. `forget_all` deletes only `.md` files and doesn't follow symlinked dirs.
- `record_outcome` read-modify-write without a lock: two decisions racing can drop one ring entry. Agent state, low; not
  reported.
- A setting changed under 500 ms before ⌘Q is lost (the frontend debounce never fires; `tear_down_and_exit` doesn't
  flush the webview). The settings store documents this as acceptable for preferences, and the "something happened"
  settings use `forceSave`. Not reported.
- `install-ids.json` unreadable or corrupt: re-mints the analytics and diagnostics ids. Cmdr's own state, low.
- `.window-state.json` unreadable: resets window geometry. Cmdr's own state, low.

## Covered

Read in full:

- `vendor/tauri-plugin-store/CMDR-PATCH.md`, `vendor/tauri-plugin-store/src/store.rs` (from `build_inner` on),
  `vendor/tauri-plugin-store/src/lib.rs` (the `load` command, builder options, plugin build and exit handler)
- `apps/desktop/src-tauri/src/state_recovery/mod.rs`, `plugin_stores.rs`, `CLAUDE.md`, `DETAILS.md`
- `apps/desktop/src-tauri/src/secrets/mod.rs`, `plain_file.rs`, `encrypted_file.rs`, `CLAUDE.md`, `DETAILS.md` (first
  part); `keychain_macos.rs` (log lines and structure)
- `apps/desktop/src-tauri/src/licensing/key_storage.rs`, `license_key.rs`, `app_status.rs` (lines 1–420), `CLAUDE.md`
- `apps/desktop/src-tauri/src/favorites/store.rs`, `CLAUDE.md`
- `apps/desktop/src-tauri/src/recents/persistence.rs`, `mod.rs` (lines 1–330), `CLAUDE.md`
- `apps/desktop/src-tauri/src/data_wipe/mod.rs`, `plan.rs`, `execute.rs` (non-test part), `CLAUDE.md`, `DETAILS.md`
- `apps/desktop/src-tauri/src/install_id.rs` (non-test part)
- `apps/desktop/src-tauri/src/window_state/mod.rs` (lines 1–200), `CLAUDE.md`
- `apps/desktop/src-tauri/src/agent/memory/store.rs`, `jail.rs`, `mod.rs`, `outcomes.rs` (lines 1–120), `CLAUDE.md`
- `apps/desktop/src-tauri/src/quit/CLAUDE.md`, `tear_down_and_exit` in `quit/mod.rs`
- `apps/desktop/src-tauri/src/config.rs` (`resolved_app_data_dir`, `app_cache_dir`, `durable_write_json`)
- `apps/desktop/src-tauri/src/lib.rs` `setup()` (lines 240–380)
- `apps/desktop/src/lib/settings/settings-store.ts`, `settings-migrations.ts`, `store-path.ts`, `CLAUDE.md`
- `apps/desktop/src/lib/shortcuts/shortcuts-store.ts` (lines 100–260), `apps/desktop/src/lib/app-status-store.ts`
  (`getStore`), `apps/desktop/src/lib/logging/logger.ts` (`getVerboseLoggingSetting`)
- `apps/desktop/src/routes/settings/+page.svelte` (init and `onDestroy`), `apps/desktop/scripts/tauri-wrapper.ts` (lines
  150–270)

## Not covered

- `agent/` beyond `agent/memory/` (about 31,000 lines: `store/` with `main.db`, `chat/`, `tools/`, `wake/`,
  `suggested_ops/`): out of reach in one pass. `main.db`'s corrupt-file recovery is documented in
  `state_recovery/DETAILS.md` but I didn't trace `AgentStore::open`.
- `settings/loader.rs`, `settings/ai_gates.rs`, `settings/restricted_shortcuts.rs`: documented read-only; skimmed via
  docs only.
- `licensing/verification.rs`, `offline_policy.rs`, `validation_client.rs`, `device_id.rs`, and `app_status.rs` past
  line 420 (trust logic rather than storage).
- `secrets/keyring_linux.rs`, `secrets/system_keychain_smb.rs`, most of `keychain_macos.rs`.
- `child_window_state.rs` (in-memory only per its docs), `window_state/geometry.rs`, `quit/mod.rs` beyond the teardown,
  `data_wipe/summary.rs`, `favorites/reach.rs`, `favorites/target.rs`.
- The frontend settings registry, applier, sections, and `restricted-settings-bridge.ts`.
- Whether `get_isolated_store_path` (`commands/`) can be made to return a path outside the data dir.
