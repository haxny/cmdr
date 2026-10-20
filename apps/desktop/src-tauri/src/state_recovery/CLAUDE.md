# State recovery

When Cmdr can't read one of its own state files, it keeps a copy, starts that state fresh, and tells the person once
(#461, the CRA's Annex I Part I (2)(f) "report on corruptions"). The security log (`../security_log/CLAUDE.md`) gets
the typed record; this module owns the copy and the notice.

## Module map

- `mod.rs`: `set_aside` (rename to `<name>.broken-<local stamp>`, keep the newest three, report), `report` (one
  security-log event plus one queued `StateRecovery`), the queue (`take_queued`), the `state-recovery-queued` nudge,
  and the test capture (`testing::capture_recoveries`, `testing::kept_copies`).
- `plugin_stores.rs`: `recover_plugin_stores`, the pre-load check of `settings.json`, `shortcuts.json`,
  `app-status.json`, and `license.json`, plus the fresh `settings.json` with the consent settings off.
- `state_file.rs`: `read_state_file` / `write_state_file`, the pair Cmdr's own JSON stores read and write through, and
  `lenient_entries`, the per-entry reader for lists of independent entries.
- IPC: `../commands/state_recovery.rs`. Frontend: `apps/desktop/src/lib/state-recovery/CLAUDE.md`.

## Must-knows

- **Every store that starts fresh goes through `set_aside` (or `report`)**, ❌ never a bare rename or delete: that's
  what keeps the copy, the record, and the notice in step. Today: the four plugin stores, `operation-log.db`, `main.db`,
  favorites, the six recents lists (`RecentEntry::STATE_FILE`), the five saved-server lists, the trusted SSH host keys,
  `usage.json`, and `ai-state.json`. What deliberately stays on a plain default: `DETAILS.md` § Per store.
- **A file that's there but didn't load is never saved over.** A JSON store reads with `read_state_file` and writes
  with `write_state_file`, ❌ never `fs::read` + `unwrap_or_default` + `durable_write_json`: unreadable (mode 000,
  `EIO`, non-UTF-8) counts as broken, and when the set-aside fails the path is held and every write refuses
  (`StateWriteError::NotLoaded`) until a read succeeds.
- **A single invalid VALUE isn't a recovery.** Only a file that doesn't parse at all (or carries an unknown schema
  version) is; a wrong value inside a good file stays the store's to drop with a log warning.
- **`recover_plugin_stores` runs in `setup()` after the instance lock and `security_log::init`, before anything loads a
  plugin store.** It sets aside a file that can't be READ (a root-owned file, an `EIO`) as well as one that doesn't
  parse: the plugin reads both as empty. If the rename fails, the patched plugin refuses every save of that store for
  the session (`Error::LoadFailed`), so the empty store never lands over the file.
- **After a `settings.json` recovery, usage stats and crash reports are OFF** (`CONSENT_OFF_AFTER_RECOVERY`), in the
  fresh file both Rust and the frontend read. Whichever way the recovery went (even a file that stays unloaded),
  `recover_settings` opens the frontend's store with consents-off defaults first and Rust's consent reads go through
  `settings_for_consent`, ❌ never a raw `settings.json` read. `operationLog.enabled` stays ON (David, 2026-10-16).
- **Once means the queue drains.** The main window takes everything at start and on each nudge; ❌ don't make the
  nudge carry the recovery, or a race between the two shows it twice.
- ❌ Never a path in the security-log event (only the `StateFile` token); the path is the notice's alone.

Flow, the per-store fallbacks, and the decisions: `DETAILS.md`.
