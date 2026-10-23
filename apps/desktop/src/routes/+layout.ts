// Tauri doesn't have a Node.js server to do proper SSR,
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false

// Register HMR crash recovery (must be outside the Svelte component so the
// listener survives layout re-evaluation during HMR)
import '#lib/hmr-recovery.js'

// Same reason, same place: an uncaught throw or unhandled rejection is otherwise
// invisible in the Rust log, in error-report bundles, and in CI.
import { registerUncaughtErrorLogging } from '#lib/logging/uncaught-errors.js'
import { startupCatalogsReady } from '#lib/intl/messages.svelte.js'

registerUncaughtErrorLogging()

/**
 * The startup gate for every window: SvelteKit renders no layout or page until this root
 * `load` settles, so the first render finds the window's starting catalogs loaded (a few
 * ms, and only when the starting language isn't English). An explicit gate rather than a
 * top-level await in the i18n runtime: see `startupCatalogsReady`.
 */
export async function load(): Promise<Record<string, never>> {
  await startupCatalogsReady
  return {}
}
