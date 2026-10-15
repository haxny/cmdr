/**
 * Runs the focus-model contract (`src/lib/test-focus-model.ts`) inside the real webview, so
 * WKWebView and WebKitGTK are checked against the same rules the unit tests check in
 * happy-dom.
 *
 * The module imports `dom-accessibility-api`, which ships no browser bundle, so this
 * bundles the two into one IIFE with Vite (once per worker) and injects it through
 * `evaluate`, the way `accessibility.spec.ts` injects axe-core. The bundle is re-injected
 * whenever the page has lost it (a reload drops `window` state).
 */

import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect } from '@playwright/test'
import { waitBudget } from '../wait-budget.js'
import type { FocusModelOptions } from '../../../src/lib/test-focus-model.js'
import type { PageLike } from './core.js'

const MODULE_PATH = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../src/lib/test-focus-model.ts')
const GLOBAL_NAME = '__cmdrFocusModel'

let bundle: Promise<string> | null = null

/** The module and its one dependency as a self-contained IIFE that sets `window.__cmdrFocusModel`. */
async function buildBundle(): Promise<string> {
  const { build } = await import('vite')
  const result = await build({
    configFile: false,
    logLevel: 'silent',
    build: {
      write: false,
      minify: false,
      lib: { entry: MODULE_PATH, formats: ['iife'], name: GLOBAL_NAME },
    },
  })
  const outputs = Array.isArray(result) ? result : [result]
  for (const output of outputs) {
    if (!('output' in output)) continue
    const chunk = output.output.find((item) => item.type === 'chunk')
    if (chunk && 'code' in chunk) return chunk.code
  }
  throw new Error('focus-model: Vite produced no chunk for test-focus-model.ts')
}

async function ensureInjected(page: PageLike): Promise<void> {
  const present = await page.evaluate<boolean>(`typeof window.${GLOBAL_NAME} === 'object'`)
  if (present) return
  bundle ??= buildBundle()
  const code = await bundle
  await page.evaluate(`(function() { ${code}\n; window.${GLOBAL_NAME} = ${GLOBAL_NAME}; return true; })()`)
}

/** Every way the element at `rootSelector` breaks the focus model, read in the page. */
export async function focusModelViolationsInPage(
  page: PageLike,
  rootSelector: string,
  options: FocusModelOptions = {},
): Promise<string[]> {
  await ensureInjected(page)
  return page.evaluate<string[]>(
    `window.${GLOBAL_NAME}.focusModelViolations(document.querySelector(${JSON.stringify(rootSelector)}), ${JSON.stringify(options)})`,
  )
}

/**
 * Fails the test, listing each violation, unless the focus model holds for `rootSelector`
 * within a short wait. A wait, not one read: a list names its cursor row a frame AFTER it
 * takes focus (`src/lib/ui/deferred-active-descendant.svelte.ts`), so the model holds a
 * beat after the flow that moved focus.
 */
export async function expectFocusModel(
  page: PageLike,
  rootSelector: string,
  options: FocusModelOptions = {},
): Promise<void> {
  await expect
    .poll(async () => focusModelViolationsInPage(page, rootSelector, options), {
      timeout: waitBudget(2000),
      message: `focus model on ${rootSelector}`,
    })
    .toEqual([])
}
