# State recovery: details

Issue #461. Must-knows in `CLAUDE.md`.

## Flow

1. A store finds its file unreadable: it doesn't parse, or it carries a schema version this build doesn't know.
2. It calls `set_aside(path, StateFile::…)`. The file moves to `<name>.broken-YYYY-MM-DDTHHMMSS` (local time; a second
   one in the same second gets `-2`), and all but the newest three copies of that file go.
3. `report` records `StateFileRecovered { file }` in the security log (`../security_log/DETAILS.md` § Event catalog)
   and queues a `StateRecovery { file, kept_copy }`. Once `init` has run it also emits `state-recovery-queued`.
4. The main window calls `take_state_recoveries` at start and on every nudge, and shows one toast per recovery
   (`apps/desktop/src/lib/state-recovery/`). Taking drains, so a recovery shows once, and a later launch with a healthy
   file finds nothing.

## Per store

- **`settings.json`, `shortcuts.json`, `app-status.json`, `license.json`** (`plugin_stores.rs`, from `setup()`): first
  sweeps the `.<name>.*.tmp` files a killed save left (the patched plugin's temp naming,
  `vendor/tauri-plugin-store/CMDR-PATCH.md`), then checks that the file parses as the JSON object the plugin reads. An
  unreadable one is set aside, so the plugin starts from no file. A recovered `settings.json` is replaced right away
  with `{"analytics.enabled": false, "updates.crashReports": false}`, written temp+rename. A file present but unreadable
  for an I/O reason is left alone: we can't tell it's damaged. Paths match their readers: the frontend's stores sit in
  `CMDR_DATA_DIR` when that's set (`resolveStorePath`), `license.json` always in the plugin's app data dir.
- **`operation-log.db`, `main.db`** (`OperationLogStore::open_setting_aside`, which `operation_log_host::start` hands
  `set_aside` through `OperationLog::open`; `AgentStore::open`): only on the typed
  `NotADatabase` / `DatabaseCorrupt` code. The DB is set aside, its `-wal` and `-shm` removed, and a fresh one built. A
  rename that doesn't happen falls back to deleting, the only way to a working DB.
- **Favorites, the recents lists**: set aside; a rename that doesn't happen deletes the file (a deleted history beats a
  broken one). Favorites reads as `Some(empty)`, never re-seeding (`../favorites/DETAILS.md`).
- **`usage.json`**: set aside; a rename that doesn't happen leaves the file, and the launch runs on an empty in-memory
  ledger.

## Decisions

- **Consent settings fail toward privacy.** A damaged `settings.json` may have held a "no" to usage stats or crash
  reports, both default on, and starting at the defaults would undo it without a word. So the fresh file says "no" to
  both, and the notice says how to turn them back on. Security-relevant settings need nothing here: their defaults are
  the safe ones (updates, cloud AI, MCP).
- **`operationLog.enabled` stays on** (David, 2026-10-16): the operation log never leaves the Mac and undo depends on
  it, so it isn't a consent-type setting.
- **A queue, not an event payload.** Most recoveries happen in `setup()`, before any window listens, so an event alone
  would be lost. The nudge carries nothing; the drain is the one source.
- **Timestamped copies, three kept.** A single rotating `.broken` copy would lose the first copy to the second
  corruption, and the first is often the more telling one. Three bound the disk a file that keeps breaking can use.
- **Its own channel, not `security_notices`.** That module's notices never carry a path; this one has to say where the
  copy went.
- **Not covered**: a `license.json` recovery loses the "key is in the secret store" marker, so the license reads as
  absent until the person enters the key again. The notice says so.
