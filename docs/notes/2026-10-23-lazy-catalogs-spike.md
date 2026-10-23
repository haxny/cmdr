# Lazy message catalogs (#435): measurements and decision record

Can a window load only English plus its own language's catalog, and still hold David's four criteria from #435? Yes, and
the per-window saving is larger than #134 estimated. David approved (a) on 2026-10-23, and it's built: branch
`worktree-agent-a3d41dbce7491ae71`. How it works now is canonical in `apps/desktop/src/lib/intl/DETAILS.md` § Loading
catalogs on demand; this note keeps the measurements and the decision trail.

## Measurements

All on David's Mac: Apple M3 Max, macOS 27.0.1 (26A434), the system JavaScriptCore / WebKit, 2026-10-23, catalogs at
`5b040918eb` (15 catalogs, 4,917 keys each once merged).

### What ships

- Catalog message text, `@key` metadata stripped, as JS object literals: 6,246 KiB total. `en` 438 KiB, a full
  translation 442–525 KiB (`ru` 702 KiB), overlays 30–154 KiB.
- Before: 9.47 MB of JS; one 6,571 KiB chunk held all 15 catalogs plus shared code, and every window imported it.
- After: 9.46 MB of JS (download and silent-update size don't change). The always-loaded chunk holding `en`, the
  manifest, and the same shared code is 546 KiB; each other locale is its own 30–702 KiB chunk.

### Method

Three harnesses, run from a scratch dir (not committed; the recipe is enough to redo them):

1. **JSC CLI** (`/System/Library/Frameworks/JavaScriptCore.framework/Versions/Current/Helpers/jsc`): a node script
   writes each locale's catalog as `Object.assign({}, {…file1…}, {…file2…}, …)`, the shape the old bundle evaluated.
   Each run is a fresh `jsc` process: read the sources first (I/O out of the timed span), `fullGC()`, time
   `(0, eval)(src)` per catalog with `preciseTime()`, drop the sources, `fullGC()`, and diff
   `memoryUsageStatistics().heapSize`. On-demand load: a `jsc -m` module that imports `en` + `de`, then times one
   `await import('./fr.mjs')`. 21 runs per scenario.
2. **WKWebView, isolated catalogs**: a 90-line Swift harness (`swiftc -O`) hosting a real `WKWebView` in a nearly
   transparent window (a windowless view gets its WebContent process throttled and stalls a second dynamic import),
   serving files through a `WKURLSchemeHandler`, the mechanism Tauri's `tauri://localhost` uses on macOS. Each run is a
   fresh harness and WebContent process. The page times `await import(...)` with `performance.now()` (1 ms resolution in
   WebKit) and posts the result; the harness reads the WebContent `phys_footprint` with
   `/usr/bin/footprint -p <pid> -f bytes` 1.5 s later (and 12 s later for a settled reading). 11 runs per mode,
   interleaved; settled readings three runs each.
3. **WKWebView, real build chunks**: the same harness importing the actually emitted chunks: the old catalog chunk vs
   the new always-loaded chunk + `de`'s chunk (each with every chunk it imports). The full app can't boot without the
   Tauri backend, so this measures module evaluation, which is exactly what the change moves.

### Results (medians, [min–max])

JSC CLI, parse + eval + merge only:

- All 15 catalogs: 39.2 ms [38.1–41.1], GC heap +9.8 MiB.
- `en` + `de`: 7.0 ms [6.8–7.3], +1.55 MiB.
- One catalog: `de` 3.6 ms, `ru` 3.5 ms (`ru` +1.0 MiB: Cyrillic strings are 16-bit).
- One more catalog on demand via dynamic import: `fr` 6.1 ms [5.8–6.6], `ru` 6.1 ms [5.5–6.9].

WKWebView, isolated catalogs (fetch through the scheme handler + parse + eval; footprint is the whole WebContent
process, 12.3 MiB with nothing loaded):

- All 15: 71 ms [69–77]; footprint 45.3 MiB at 1.5 s, 33.0 MiB settled.
- `en` + `de`: 15 ms [15–15]; footprint 18.5 MiB at 1.5 s, 16.4 MiB settled.
- One more catalog on demand with `en` + `de` loaded: `fr` 7 ms [6–8], `ru` 8 ms [7–9]; the next animation frame after
  the load lands 20 ms [15–23] after the switch started (frame alignment included).

WKWebView, real build chunks:

- Old catalog chunk: 99 ms [97–101]; footprint 56.6 MiB at 1.5 s, 43.7 MiB settled.
- New `en` chunk + `de`: 30 ms [30–35]; 23.3 MiB at 1.5 s, 19.8 MiB settled.
- Plus switching to `fr`: +6 ms; 26.1 MiB at 1.5 s, 21.6 MiB settled.

So, per window (main, Settings, viewer, queue, and shortcuts each run their own WebContent process): about 70 ms less
module evaluation when the window opens, about 33 MiB less footprint in its first seconds, and about 24 MiB less once
settled. Main + Settings open: roughly 48 MiB. #134 guessed 10–20 MB.

Caveats: Tauri's scheme handler does more per request than the harness's (a Rust asset lookup, possibly decompressing an
embedded asset), so the real on-demand load is likely a few ms above 7–8 ms. Not measured inside the running app: the
prod app exposes no JS entry point, and a dev build in the spike worktree would have meant a cold Rust build.

## The four criteria, as built

1. **No restart to switch language: holds.** Same live path as before (`appearance.language` change → `applyLanguage()`
   / `initWindowLanguageSync()` → `setLocale()`), now returning a promise.
2. **No flash of English or a blank frame: holds.** `setLocale()` loads the chain, then switches in one synchronous
   step; the old language stays on screen until then, and the latest call wins. The picker already applies the
   highlighted row, so the highlight IS the preload; no separate preload code. Startup is better than before: a new
   window loads and speaks the catalog the last switch landed on from its first render.
3. **Settings search covers current language + English: holds.** Both catalogs are always in every window. The one
   cross-language read (the Language row's label in every language) comes from the always-loaded manifest.
4. **Settings search finds Language by "Deutsch", "Magyar", "Nyelv", or "German" in any UI language: holds.** David
   scoped this to Settings search, not the picker (2026-10-23), so the picker is unchanged. Checked on `main` before the
   change, across `en`, `fr`, `de`, `hu`, `ru`, `zh-Hant`: Deutsch, Magyar, and Nyelv found it in all six; German found
   it in none. Fixed with `localeEnglishName()` (CLDR via `Intl.DisplayNames(['en'])`, no catalog needed);
   `settings-search.i18n.test.ts` pins all 24 combinations.

## Decisions taken while building

- **A Vite plugin, not a lazy `import.meta.glob`**: a glob makes one chunk per FILE (39 × 14), so a switch would be
  dozens of requests, and the per-tag merge and shipped list would move into the runtime.
- **The startup catalog is a localStorage hint, not a Rust-injected value**: every window shares the origin, it's
  synchronous, and a wrong hint costs one ~8 ms load. Rust injection would need the setting resolved before every
  webview's creation, including windows the frontend opens.
- **Eviction after every switch**: arrowing through the picker would otherwise leave all 15 in memory until restart.
- **`tStringIn()` throws on an unloaded catalog outside production**: otherwise a cross-language read quietly answers in
  English and looks fine in review.
- **The runtime logs through a lazily imported logger**: a static import pulled IPC and the settings store into every
  test file through `test-setup.ts`, and broke a test that mocks the store path.

## Risks that remain

- **The startup gate**: every window's first render waits for one catalog fetch (~8 ms, against ~70 ms of evaluation
  saved), through the root `+layout.ts` `load`. A chunk that fails to load is caught and logged: the window starts in
  its webview language, and a switch keeps the old one. The first version used a top-level await instead, and WebKitGTK
  ran secondary windows' page components before their modules had finished evaluating (every Settings, viewer, and queue
  window crashed in CI, run 39989092928); top-level await is now banned in app code by ESLint.
- **The menu bar can switch a few ms before the webview**: `setUiLanguage` (Rust) fires immediately while the webview
  awaits its chunk.
- **A stale startup hint shows the hinted language for a few ms** before the setting applies, the same class as the old
  "webview language until the setting applies", never English unless English was the hint.
- **No download-size win**: the bundle stays 9.46 MB. The win is per-window parse time and memory.
- **E2E**: CI's Playwright lanes run the real E2E build, so they exercise the startup wait, the main window's settings
  step awaiting its switch, and every secondary window loading its own catalog. They don't exercise the capture API's
  `setLocale`, which now returns the switch (`captureCall` returns what `page.evaluate` resolves, the path
  `setTextSize`'s promise already takes): only `pnpm i18n:shots:overflow` drives it, so the next overflow capture is its
  first real run.
