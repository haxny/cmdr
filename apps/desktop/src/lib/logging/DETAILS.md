# Logging details

Depth and rationale for the frontend logging bridge. `CLAUDE.md` holds the must-knows. The Rust dispatch tree is
documented in `src-tauri/src/logging/CLAUDE.md` (canonical home for the fern/per-output mechanism).

## Architecture

```
getAppLogger('feature')
  -> LogTape (separate level gates per sink)
    -> console sink (browser devtools): info+ in dev, error+ in prod, debug+ with verbose
    -> tauriBridge sink: debug+ in dev (RUST_LOG filters on Rust side); in prod warn+ for every category
       and debug+ for debugCategories; debug+ with verbose
        -> batch for 100ms, dedup consecutive identical entries, throttle 200/s
        -> invoke('batch_fe_logs', entries[])
          -> Rust: log::info!(target: "FE:feature", msg)
            -> fern Dispatch tree (src-tauri/src/logging/DETAILS.md has its shape)
```

## Viewing the debug log

`open-debug-log.ts` serves `help.viewDebugLog` from both the palette and the Help menu. It asks the typed
`getDebugLogPath` wrapper for the backend’s resolved path, then opens the existing file viewer against the local volume,
tailed (`{ tail: true }`): it starts at the end and follows new lines live. A missing path produces an informational
toast; a rejected IPC/window call produces a retry toast and a diagnostic log. Path selection lives in
`apps/desktop/src-tauri/src/logging/DETAILS.md`.

## Crashes SvelteKit catches

A component that throws while rendering (or a `load` that throws) never reaches the `window` `error` listener: SvelteKit
catches it and swaps the window for its error page. A v0.54.0 user on Catalina got exactly that, a bare "500 / Internal
Error" and a log with no trace of why. So:

- `apps/desktop/src/hooks.client.ts` implements `handleError`. For an `unknown` or `app` throw it calls `logPageCrash`
  (`Page crashed in {routeId}: {detail}\n{stack}`, error level, so an opted-in auto-send ships it; `detail` and the
  frames come from `describeThrown`, the same split the `window` listeners use, cause chain included) and returns
  `{ detail }`, plus `message: 'Name: message'` for an `unknown` throw. A `framework` error (a route that doesn't exist)
  isn't a crash and isn't logged.
- `apps/desktop/src/routes/+error.svelte` shows `page.error.detail` and offers a report (`pageCrash` incident) plus
  Reload. Only the root layout sits above it, so in the main window it hosts the report dialog and toasts itself.
- **`whenLoggerReady` holds early lines.** LogTape drops anything logged before `configure`, and `initLogger` runs in
  the root layout's `onMount`, which comes after a child's first render. A first-render crash is exactly that early, so
  `logPageCrash` and both `window` listeners emit through `whenLoggerReady`, which queues until `initLogger` has
  configured LogTape, then drains. `logger-ready.test.ts` pins it.

## Decisions

- **LogTape on the frontend**: it gives the `getAppLogger()` API, hierarchical categories, and per-feature debug toggles
  (`debugCategories`); only the sink is ours.
- **Custom batch IPC instead of the plugin JS API**: the bridge batches into one IPC call per 100 ms, with dedup and
  throttle (critical for infinite-loop protection).
- **The production gate sits at warn for every category.** The Rust file target stays Debug precisely so a bundle
  carries context, and a frontend gate at error would hide nearly every frontend warn from it, since only
  `debugCategories` pass below the root gate. Info and debug stay gated for volume. Two costs follow from warn being
  bundle content: an expected, recoverable condition logs at info (the archive-password prompt in
  `transfer-progress-state.svelte.ts` is the standing case, and a warn there queued it into bundles), and a failure that
  recurs per event, poll tick, or call goes through `LogOnceGate` (`log-once.ts`), or one broken store read fills the
  file. Promoting a line to error instead is not free: `batch_fe_logs` routes Error through `log_error!`, which feeds
  the error-report auto-send dispatcher.
- **`parentSinks: 'override'` on the `debugCategories` children** (verified on LogTape 2.3.0, `dist/logger.js`
  `createSinkDispatchPlan`, 2026-09-13). The default `inherit` unions the parent's sinks with the child's own for every
  level the parent passes, and the children list the same two sinks, so each such line reached each sink twice: a real
  prod log read `ERROR FE:fileExplorer … (x2, deduplicated)`. `logger.test.ts` configures LogTape with the real
  `buildLoggerConfig` and counts deliveries per sink, per mode.
- **`debugCategories` doesn't reach the devtools console**: its sink filters on its own level (info in dev), which a
  logger's `lowestLevel` can't lower. Use the verbose toggle to see debug lines there.
- **Hand-rolled fern dispatch instead of `tauri-plugin-log`**: the plugin routes everything through one shared level. We
  need per-output filtering (file at Debug for error reports, terminal at Info for clean dev output). fern's tree of
  `Dispatch` chains makes this trivial. Full mechanism in `src-tauri/src/logging/CLAUDE.md`.
- **`RUST_LOG` parsed into `level_for()` on the stdout chain only**: the file chain stays Debug regardless, so
  `RUST_LOG=cmdr_lib::network=debug,smb=warn,info` controls the terminal without affecting error-report bundle content.
- **`developer.verboseLogging`**: with per-output filtering, the toggle bumps the stdout chain from Info to Debug at
  runtime via an `AtomicU8` (no dispatch rebuild, no records lost). The file target stays Debug, so error-report content
  is unchanged.
- **Rotation, the log-size cap, and the restart-to-apply transitions are the Rust side's**, single-sourced in
  `apps/desktop/src-tauri/src/logging/DETAILS.md`. Nothing on the frontend reads or reconfigures them.
- **The bridge carries the rendered message and not `record.properties`, and a lint rule closes the gap rather than the
  bridge widening.** `cmdr/no-unrendered-log-fields` fails the build on a property the template never names. The
  alternative considered was serializing the property bag into the entry, and it loses on three counts:
  `JSON.stringify(new Error(...))` is `{}` (the non-enumerable `message` and `stack` drop out), so the single most
  common payload would arrive empty; arbitrary values need depth and cycle caps, and inflate a file target that is
  always Debug under a 200 MB roof; and an uploaded bundle's redaction contract (`src-tauri/src/redact/`) is reasoned
  about over strings a developer wrote, not over whatever an object happens to hold. Making the author name what matters
  also reads better at triage time than an appended blob. Cost: the fix at a call site is a judgment call, not a
  mechanical one.
- **Every rendered value goes through `describeForLog` (`describe-for-log.ts`), and call sites pass values raw.** The
  bridge used to render with `String(part)`, and about 290 call sites pre-stringified on top (`String(e)`, the
  `e instanceof Error ? e.message : String(e)` ternary), so a plain object logged as `[object Object]` and an `Error`
  lost its cause and a `TypedFailure` its payload. ERR-VEZUB, a `DeleteDialog` render crash, reached its report as
  `detail="[object Object]"`. One seam renders: strings verbatim, an `Error` as `Name: message; caused by …` (cycle- and
  depth-guarded), a `TypedFailure` plus its `.failure` as JSON, a class with its own `toString` (`URL`) through it, and
  anything else as COMPACT JSON with a cycle guard, capped at 1,000 chars. Compact on purpose: inside a `detail="…"`
  field the report redactor tokenizes `"path":"…"`, `"server":"…"`, and the other identity keys by matching exactly that
  shape (`src-tauri/src/redact/detail.rs` `redact_json_identity_pairs`), so a path inside a logged object is still
  redacted. Anything that must touch every logged value (a redaction marker) belongs here.
  `cmdr/no-stringified-log-values` keeps call sites raw: it autofixes `String(x)` and the ternary in a property value to
  `x`, and a `${String(x)}` in a template-literal message to a `{x}` placeholder (`{error}` for a caught value, so it
  lands in a redacted `detail` field).
- **`apps/desktop/src/hooks.client.ts` exists for the errors SvelteKit catches.** Without a `handleError` hook,
  SvelteKit's `transformError` hands every `<svelte:boundary>` `{ status: 500, message: 'Internal Error' }`
  (`@sveltejs/kit` `runtime/client/client.js` `handle_error`, verified on 3.1.0, 2026-10-19), so `DialogManager`'s
  boundary could never say what a dialog threw. For an `unknown` throw the hook also returns
  `{ message: 'Name: message' }` (SvelteKit merges it over its fallback), which is what the boundary's `onerror`
  receives; § Crashes SvelteKit catches has the logging side.
- **A structured-log pipeline is the thing that would change this answer.** If logs ever need to be queried rather than
  read, carrying properties end to end becomes the right shape, and the rule becomes redundant. That is a different
  project, not an incremental widening of the bridge.
