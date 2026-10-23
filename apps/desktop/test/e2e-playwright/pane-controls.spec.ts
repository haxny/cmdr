/**
 * Pane chrome from the keyboard: "Move focus to the pane controls" puts focus on the focused
 * pane's volume button, the arrows walk and hop between the pane's controls, activating a
 * path segment navigates and lands back in the list, Escape returns to the list, and `Tab`
 * stays the pane switch. `pane/pane-chrome.ts` is the mechanism.
 *
 * Every press here is synthetic (`DETAILS.md` § "Keyboard: no press in this suite is a real
 * one"): it lands on `document.activeElement`, so the keys reach the control focus sits on,
 * but no press triggers a browser default. Enter's default (clicking the focused button) is
 * therefore stood in for by `.click()` on the focused element.
 */

import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import {
  dispatchMenuCommand,
  ensureAppReady,
  executeViaCommandPalette,
  expectArrowDownMovesCursorIn,
  expectFocusModel,
  fileExistsInFocusedPane,
  focusPane,
  type PageLike,
} from './helpers.js'

const FOCUSED_LIST = '.file-pane.is-focused [role="listbox"]'
const LOCATION_BAR = '.file-pane.is-focused [role="toolbar"][data-pane-chrome]'

/** What focus sits on, as a short selector-ish description, for poll messages. */
async function focused(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function() {
    var el = document.activeElement;
    if (!el) return 'nothing';
    var where = el.closest('.tab-bar') ? 'tabs' : el.closest('.header') ? 'location' : el.closest('.header-row') ? 'sort' : 'elsewhere';
    return where + ':' + el.tagName.toLowerCase() + '.' + Array.prototype.join.call(el.classList, '.');
  })()`)
}

/** Whether focus sits in the focused pane's own chrome group matching `selector`. */
async function focusIsIn(tauriPage: PageLike, selector: string): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function() {
    var wrapper = document.querySelector('.file-pane.is-focused')?.closest('[data-pane-chrome-scope]');
    var el = document.activeElement;
    return !!wrapper && !!el && wrapper.contains(el) && !!el.closest(${JSON.stringify(selector)});
  })()`)
}

async function enterControls(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'pane.focusControls')
  await expect
    .poll(async () => focused(tauriPage), { timeout: waitBudget(3000) })
    .toMatch(/^location:button\.volume-name/)
}

test.describe('Pane controls from the keyboard', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await focusPane(tauriPage, 0)
  })

  test('the command lands on the volume button, and Escape goes back to the list', async ({ tauriPage }) => {
    await enterControls(tauriPage)
    // Escape from the controls IS the keyboard path under test (no overlay is open), so it's a press.
    await tauriPage.keyboard.press('Escape')
    await expectFocusModel(tauriPage, FOCUSED_LIST)
    await expectArrowDownMovesCursorIn(tauriPage, 'left')
  })

  // The palette restores focus when it closes, after the command already ran.
  test('run from the command palette, focus stays on the volume button', async ({ tauriPage }) => {
    await executeViaCommandPalette(tauriPage, 'Move focus to the pane controls')
    await expect
      .poll(async () => focused(tauriPage), { timeout: waitBudget(3000) })
      .toMatch(/^location:button\.volume-name/)
  })

  test('the arrows walk the location bar and hop to the sort header and the tab bar', async ({ tauriPage }) => {
    await enterControls(tauriPage)
    await tauriPage.keyboard.press('ArrowRight')
    await expect.poll(async () => focused(tauriPage), { timeout: waitBudget(3000) }).toMatch(/^location:button\./)
    expect(await focused(tauriPage)).not.toMatch(/volume-name/)

    await tauriPage.keyboard.press('ArrowDown')
    await expect.poll(async () => focusIsIn(tauriPage, '.header-row'), { timeout: waitBudget(3000) }).toBe(true)

    await tauriPage.keyboard.press('ArrowUp')
    await tauriPage.keyboard.press('ArrowUp')
    await expect
      .poll(async () => focusIsIn(tauriPage, '.tab-bar [role="tab"][aria-selected="true"]'), {
        timeout: waitBudget(3000),
      })
      .toBe(true)
  })

  // The Full date header's Sort by chevron is a plain button in the sort toolbar, left of the
  // date header, so the walk reaches it with no `tabindex`. (Its native menu can't be driven here.)
  test('the arrows reach the date Sort by chevron in Full view', async ({ tauriPage }) => {
    await executeViaCommandPalette(tauriPage, 'Full view')
    await expect
      .poll(async () => tauriPage.isVisible('.file-pane.is-focused .full-list-container .date-sort-chevron'), {
        timeout: waitBudget(5000),
      })
      .toBeTruthy()

    await enterControls(tauriPage)
    await tauriPage.keyboard.press('ArrowDown')
    await expect.poll(async () => focusIsIn(tauriPage, '.header-row'), { timeout: waitBudget(3000) }).toBe(true)
    await tauriPage.keyboard.press('End')
    await tauriPage.keyboard.press('ArrowLeft')
    await expect
      .poll(async () => focused(tauriPage), { timeout: waitBudget(3000) })
      .toMatch(/^sort:button\.date-sort-chevron/)
    expect(await tauriPage.evaluate<string | null>(`document.activeElement.getAttribute('aria-haspopup')`)).toBe('menu')
  })

  test('a path segment navigates, and focus lands back in the list', async ({ tauriPage }) => {
    await enterControls(tauriPage)
    // The last segment button is the current folder's parent: the fixture root, holding `left/`.
    // Walk → onto it; a drive badge may sit between the volume button and the path.
    const onLastSegment = async (): Promise<boolean> =>
      tauriPage.evaluate<boolean>(
        `document.activeElement === Array.prototype.slice.call(document.querySelectorAll('${LOCATION_BAR} .path-segment')).pop()`,
      )
    for (let i = 0; i < 20 && !(await onLastSegment()); i++) await tauriPage.keyboard.press('ArrowRight')
    expect(await onLastSegment(), `focus walked past the path to ${await focused(tauriPage)}`).toBe(true)

    // Enter's default action on a focused button, which a synthetic press doesn't trigger.
    await tauriPage.evaluate(`document.activeElement.click()`)
    await expect
      .poll(async () => fileExistsInFocusedPane(tauriPage, 'left'), { timeout: waitBudget(5000) })
      .toBeTruthy()
    await expectFocusModel(tauriPage, FOCUSED_LIST)
  })

  test('Tab from the controls switches panes', async ({ tauriPage }) => {
    await enterControls(tauriPage)
    await tauriPage.keyboard.press('Tab')
    await expect
      .poll(
        async () =>
          tauriPage.evaluate<boolean>(`document.querySelectorAll('.file-pane')[1]?.classList.contains('is-focused')`),
        { timeout: waitBudget(3000) },
      )
      .toBe(true)
    // `right/` is empty, so its list's `..` row is what the focus model checks.
    await expectFocusModel(tauriPage, FOCUSED_LIST)
  })
})
