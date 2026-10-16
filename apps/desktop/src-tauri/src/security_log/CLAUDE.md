# Security event log

A small, local, append-only record of security-relevant changes and refusals, `security-events.jsonl` in the app data
dir, with an opt-out (`securityLog.enabled`, Settings › Updates & privacy). The CRA's Annex I Part I (2)(l).

## Module map

- `mod.rs`: `record` (the ONE emitter), `subscribe`, the opt-out (`Recorder`), `on_policy_applied`, `init`, `on_exit`,
  and the test capture (`testing::capture_events`).
- `types.rs`: `SecurityEvent` and its typed fields, `LogLine`. `writer.rs`: the background writer over
  `logging::rotating_file`. `episodes.rs`: per-episode coalescing for repeating rejections, plus its closer thread.
- IPC: `../commands/security_log.rs`. Frontend: the Security events card in `UpdatesSection.svelte`.

## Must-knows

- **Typed end to end.** A `kind` or enum token is the file format: renaming a TOKEN is a format change, renaming a
  variant is free. ❌ Never a free-text field or a `String` from an error's `Display`; `types.rs` tests pin each
  variant's fields, so a new one fails until it's reviewed.
- **One emitter, many consumers.** Every source calls `record`. Subscribers hear every event whatever the opt-out says
  (a notice isn't the record), plus every repeat inside an open episode (`Heard::Repeat`); only the file obeys the
  opt-out, and never sees a repeat. ❌ Don't add a second event enum for a new consumer.
- **Never blocks or fails the caller.** A failed write drops that one line with a warning.
- **The writer checks `data_wipe::has_wiped()` before every write**, so a line recorded at quit can't recreate the file
  after "Delete all Cmdr data". ❌ It never creates the data dir either.
- **Turning off writes one last "turned off" line**; turning on writes "turned on" first. A policy requiring the log
  overrides the switch, and its own change is recorded while the log is still on.
- **Repeating rejections go through `note_*_rejection`, ❌ never `record` per request**: one "started", then one
  "ended" with the count after 10 quiet minutes or at quit.
- **It lives in the data dir, not the log dir**, which keeps it out of error-report bundles. ❌ Don't move it.
- **`init` runs before `managed_policy::init`** in `setup()`; earlier events have nowhere to go.

Event catalog, hook points, opt-out rules, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
