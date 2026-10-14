/**
 * Unit tests for `focusModelViolationsInPage`: the Vite bundle of `test-focus-model.ts` and
 * its `dom-accessibility-api` dependency builds, injects through `evaluate`, and answers
 * the same as the module does when imported. The payloads run verbatim against happy-dom
 * through a `PageLike` stub, so the program under test is the one the suite ships.
 */

import { afterEach, describe, expect, it } from 'vitest'
import type { PageLike } from './core.js'
import { focusModelViolationsInPage } from './focus-model.js'

/** Every `evaluate` payload the helper sent, so the injection can be counted. */
let payloads: string[] = []

/** A `PageLike` running each payload against happy-dom, the way the webview would. */
const page = {
  evaluate: (js: string): Promise<unknown> => {
    payloads.push(js)
    // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the evaluate payload IS the code under test; the whole point is to run it verbatim.
    const run = new Function(`return ${js}`) as () => unknown
    return Promise.resolve(run())
  },
} as unknown as PageLike

function mountList(): void {
  document.body.innerHTML = `
    <div class="root">
      <div role="listbox" tabindex="-1" aria-label="Files" aria-activedescendant="row-1">
        <div role="option" id="row-0">a.txt</div>
        <div role="option" id="row-1">b.txt</div>
      </div>
    </div>`
  document.querySelector<HTMLElement>('[role="listbox"]')?.focus()
}

afterEach(() => {
  document.body.innerHTML = ''
  payloads = []
  delete (window as unknown as Record<string, unknown>).__cmdrFocusModel
})

describe('focusModelViolationsInPage', () => {
  it('injects the bundle once and passes a sound list', async () => {
    mountList()
    expect(await focusModelViolationsInPage(page, '.root')).toEqual([])
    const before = payloads.length
    expect(await focusModelViolationsInPage(page, '.root')).toEqual([])
    // The second call only probes for the bundle and runs the check: no re-injection.
    expect(payloads.length - before).toBe(2)
  }, 30_000)

  it('reports a violation the bundled name computation finds', async () => {
    mountList()
    const row = document.getElementById('row-1')
    if (row) row.textContent = ''
    expect(await focusModelViolationsInPage(page, '.root')).toEqual([expect.stringContaining('has no accessible name')])
  }, 30_000)

  it('passes the options through', async () => {
    mountList()
    document.querySelector<HTMLElement>('.root')?.setAttribute('tabindex', '0')
    document.querySelector<HTMLElement>('.root')?.focus()
    expect(await focusModelViolationsInPage(page, '.root', { focusMayBeContainer: true })).toEqual([])
    expect(await focusModelViolationsInPage(page, '.root', { liveRegions: ['.announcer'] })).toEqual([
      expect.stringContaining('has focus and rows, but no aria-activedescendant'),
      expect.stringContaining('.announcer matches 0 elements'),
    ])
  }, 30_000)
})
