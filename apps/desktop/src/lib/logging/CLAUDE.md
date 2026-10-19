# Logging

Unified logging: frontend (Svelte/TS) and backend (Rust) logs share one terminal stream and log file with unified
timestamps. The Rust side's fern dispatch tree, with its per-output level filtering, is documented in
`apps/desktop/src-tauri/src/logging/CLAUDE.md`.

## Module map

- **`open-debug-log.ts`**: Help and palette entry point for the read-only debug log viewer.
- **`logger.ts`**: LogTape config, `getAppLogger()` entry point, verbose toggle, `debugCategories`.
- **`log-bridge.ts`**: batching sink (collects FE logs for 100 ms, dedups, throttles at 200/s, sends to Rust via IPC).
- **`describe-for-log.ts`**: renders every logged value.
- **`log-markers.ts`**: marks a value for error reports, mirroring `cmdr_fs::log_private` (table: `log-markers.json`).
- **`uncaught-errors.ts`**: forwards `window` `error` / `unhandledrejection` to `log.error` under the `uncaught`
  category (registered from `routes/+layout.ts`); `logPageCrash` does it for throws SvelteKit catches
  (`apps/desktop/src/hooks.client.ts`).
- **`log-once.ts`**: `LogOnceGate`, which lets a failure that recurs per event, poll tick, or call log once until it
  clears.
- Rust side: `src-tauri/src/commands/logging.rs` (batch IPC receiver + runtime level control); the dispatch tree is in
  `src-tauri/src/logging/CLAUDE.md`.

Usage (adding logging, `RUST_LOG` recipes, the verbose toggle): `docs/tooling/logging.md`.

## Must-knows

- **FE logs use `FE:{category}` as their log target**, so `RUST_LOG=FE:viewer=debug,info` filters them on the Rust side.
- **In production, warn and above from every category reach the log file and every error-report bundle**; info and debug
  don't, except for `debugCategories`, which reach them at debug. So a warn is bundle content: an expected condition
  logs at info, and a failure that can repeat per event goes through `LogOnceGate`. ❗ The `debugCategories` children
  need `parentSinks: 'override'`, or each line reaches each sink twice (`logger.test.ts` pins the counts). Dev sends
  debug+ to Rust for `RUST_LOG` to filter; the devtools console keeps its own gate, which only the verbose toggle lifts.
- **The bridge forwards the rendered message and nothing else, so every property must appear in the template.**
  `FrontendLogEntry` is level + category + message; nothing reads `record.properties`, so a field the message never
  names reaches neither the log file nor a bundle. `cmdr/no-unrendered-log-fields` enforces it at every level.
- **Pass logged values raw: `describeForLog` is the one place a value becomes text** (cause chains, `TypedFailure`
  payloads, compact JSON with identity-keyed values marked). `cmdr/no-stringified-log-values` autofixes `String(x)`.
- **Name a path or identity placeholder from `REDACTOR_KEY_BY_PLACEHOLDER`** (`{path}`, `{name}`, `{volumeId}`, …): it
  picks the `key="…"` field and the marker kind a report hashes the value by. ❌ Never a generic name or spliced in.
- **The markers mirror Rust exactly**, pinned by `src-tauri/src/redact/fe_bridge_tests.rs` against
  `bridge-redaction-cases.json` (a bridge output change updates that golden). ❌ A marked string is log text: never
  show, store, or send it.
- **The bridge dedups and throttles to protect against infinite-loop log floods.** Consecutive identical messages get
  ` (×N, deduplicated)`; above 200 FE logs/s the excess drops with an "Excessive frontend logging detected" warning.
  Don't remove these guards.
- **An uncaught frontend throw is only visible because `uncaught-errors.ts` forwards it.** The console is unread under
  Tauri, so without those listeners a crash leaves no line in the log file, a bundle, or a CI E2E run (render throws and early lines: `DETAILS.md` § Crashes SvelteKit catches). They observe and
  never `preventDefault` (that would also blind `hmr-recovery`). ❗ WebKit's `error.stack` carries FRAMES ONLY, so the
  message is put back in front of it; ❌ never log a stack verbatim here. The message rides `{detail}`, the frames
  `{stack}`.
- **`beforeunload` flush is best-effort (async)**, so logs right before a page unload may not all reach Rust.
- **Error-report bundles (Help > Send error report…) include the file target's recent debug logs**, the same logs the
  cap setting governs. Keep the file chain at Debug.

Architecture and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
