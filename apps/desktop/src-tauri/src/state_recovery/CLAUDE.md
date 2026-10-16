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
- IPC: `../commands/state_recovery.rs`. Frontend: `apps/desktop/src/lib/state-recovery/CLAUDE.md`.

## Must-knows

- **Every store that starts fresh goes through `set_aside` (or `report`)**, ❌ never a bare rename or delete: that's
  what keeps the copy, the record, and the notice in step. Today: the four plugin stores, `operation-log.db`, `main.db`,
  favorites, the six recents lists (`RecentEntry::STATE_FILE`), and `usage.json`.
- **A single invalid VALUE isn't a recovery.** Only a file that doesn't parse at all (or carries an unknown schema
  version) is; a wrong value inside a good file stays the store's to drop with a log warning.
- **`recover_plugin_stores` runs in `setup()` after the instance lock and `security_log::init`, before anything loads a
  plugin store.** The plugin reads an unparseable file as empty and its next save would overwrite the evidence.
- **After a `settings.json` recovery, usage stats and crash reports are OFF** (`CONSENT_OFF_AFTER_RECOVERY`), in the
  fresh file both Rust and the frontend read. `operationLog.enabled` stays at its default ON (David, 2026-10-16).
- **Once means the queue drains.** The main window takes everything at start and on each nudge; ❌ don't make the
  nudge carry the recovery, or a race between the two shows it twice.
- ❌ Never a path in the security-log event (only the `StateFile` token); the path is the notice's alone.

Flow, the per-store fallbacks, and the decisions: `DETAILS.md`.
