/**
 * E2E for Synchronize folders (⇧F12): two real folders, a Mirror run through the confirm, and the re-compare after.
 *
 * Fixture (made here, under the shared `left/` and `right/`): `left/sync-src` holds `a.txt` (older) and `b.txt`;
 * `right/sync-dst` holds a NEWER `a.txt` and `c.txt`. Mirror → therefore replaces a newer file with an older one
 * (the "Older replaces newer" warning), copies `b.txt`, and removes `c.txt`. With `sync.keep.policy` at
 * `versionsAlways`, the replaced and removed files land in the target's versions folder, never deleted.
 *
 * The dialog's own keys are ⌘ combos on every platform (`src/lib/sync/sync-keys.ts` matches `metaKey`), so this spec
 * sends `Meta` rather than `CTRL_OR_META`; the keys are dispatched as DOM events, which reach it on Linux too.
 */

import fs from 'fs'
import path from 'path'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureMcpClient, mcpCall, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import {
  clickButtonByText,
  dispatchMenuCommand,
  drainOperations,
  ensureAppReady,
  getFixtureRoot,
  pointerClick,
  pressKey,
  waitForTransferUiToSettle,
  type PageLike,
} from './helpers.js'

const DIALOG = '[data-dialog-id="sync-folders"]'
const CONFIRM = '[data-dialog-id="sync-confirm"]'
const OLD = new Date('2026-01-01T10:00:00Z')
const NEW = new Date('2026-06-01T10:00:00Z')

function write(file: string, content: string, mtime: Date): void {
  fs.mkdirSync(path.dirname(file), { recursive: true })
  fs.writeFileSync(file, content)
  fs.utimesSync(file, mtime, mtime)
}

/** Every file under `dir`, relative, sorted (the versions folder's own dated name included). */
function filesUnder(dir: string): string[] {
  if (!fs.existsSync(dir)) return []
  return (fs.readdirSync(dir, { recursive: true }) as string[])
    .filter((rel) => fs.statSync(path.join(dir, rel)).isFile())
    .map((rel) => rel.split(path.sep).join('/'))
    .sort()
}

async function dialogText(page: PageLike, selector: string): Promise<string> {
  return page.evaluate<string>(
    `(function(){ var el = document.querySelector(${JSON.stringify(selector)}); return el ? el.textContent || '' : '' })()`,
  )
}

/** The accessible names of the grid's data rows, top to bottom. */
async function rowLabels(page: PageLike): Promise<string[]> {
  return page.evaluate<string[]>(`(function(){
    var rows = document.querySelectorAll(${JSON.stringify(`${DIALOG} [role="grid"] .column-list-row.is-data`)});
    return Array.prototype.map.call(rows, function (row) { return row.getAttribute('aria-label') || '' });
  })()`)
}

async function clickRow(page: PageLike, name: string): Promise<void> {
  const outcome = await pointerClick(
    page,
    `Array.prototype.find.call(document.querySelectorAll(${JSON.stringify(`${DIALOG} .column-list-row.is-data`)}),
      function (row) { return (row.getAttribute('aria-label') || '').indexOf(${JSON.stringify(`${name},`)}) === 0 }) || null`,
  )
  expect(outcome).toBe('clicked')
}

let src = ''
let dst = ''

test.beforeEach(() => {
  const root = getFixtureRoot()
  recreateFixtures(root)
  src = path.join(root, 'left', 'sync-src')
  dst = path.join(root, 'right', 'sync-dst')
  write(path.join(src, 'a.txt'), 'older on the left', OLD)
  write(path.join(src, 'b.txt'), 'only on the left', OLD)
  write(path.join(dst, 'a.txt'), 'newer on the right!', NEW)
  write(path.join(dst, 'c.txt'), 'only on the right', OLD)
})

test.afterEach(async ({ tauriPage }) => {
  await drainOperations(tauriPage)
  await mcpCall('set_setting', { id: 'sync.keep.policy', value: 'trashElseVersions' })
  fs.rmSync(src, { recursive: true, force: true })
  fs.rmSync(dst, { recursive: true, force: true })
  restoreFixtureTree(getFixtureRoot())
})

test.describe('Synchronize folders', () => {
  test.describe.configure({ timeout: waitBudget(60_000) })

  test('Mirror over a newer target needs the explicit button, keeps what it replaced, and re-compares equal', async ({
    tauriPage,
  }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await mcpCall('set_setting', { id: 'sync.keep.policy', value: 'versionsAlways' })
    await mcpNavToPath('left', src)
    await mcpNavToPath('right', dst)

    // ⇧F12, through the command it's bound to.
    await dispatchMenuCommand(tauriPage, 'sync.openDialog')
    await expect
      .poll(() => dialogText(tauriPage, DIALOG), { timeout: waitBudget(8000) })
      .toContain('Synchronize folders')

    // Mirror →, then compare again (an option never re-compares on its own).
    const mirror = await pointerClick(
      tauriPage,
      `Array.prototype.find.call(document.querySelectorAll(${JSON.stringify(`${DIALOG} [role="tab"]`)}),
        function (tab) { return (tab.textContent || '').trim() === 'Mirror →' }) || null`,
    )
    expect(mirror).toBe('clicked')
    await clickButtonByText(tauriPage, `${DIALOG} button`, 'Compare again')
    await expect
      .poll(async () => (await rowLabels(tauriPage)).join('\n'), { timeout: waitBudget(10_000) })
      .toContain('c.txt,')
    const labels = await rowLabels(tauriPage)
    expect(labels.find((label) => label.startsWith('a.txt,'))).toContain('will be copied to the right')

    // Override one row: skip a.txt, then ⌘L copies it to the right after all, through the backend.
    await clickRow(tauriPage, 'a.txt')
    await pressKey(tauriPage, 'Meta+n', 'KeyN')
    await expect
      .poll(async () => (await rowLabels(tauriPage)).find((label) => label.startsWith('a.txt,')) ?? '', {
        timeout: waitBudget(5000),
      })
      .not.toContain('will be copied to the right')
    await pressKey(tauriPage, 'Meta+l', 'KeyL')
    await expect
      .poll(async () => (await rowLabels(tauriPage)).find((label) => label.startsWith('a.txt,')) ?? '', {
        timeout: waitBudget(5000),
      })
      .toContain('will be copied to the right')

    // ⌘↩: the confirm lists the older-replaces-newer row first, and Enter alone won't run it.
    await pressKey(tauriPage, 'Meta+Enter')
    await expect
      .poll(() => dialogText(tauriPage, CONFIRM), { timeout: waitBudget(8000) })
      .toContain('Older replaces newer')
    expect(await dialogText(tauriPage, CONFIRM)).toContain('a.txt: left')
    await pressKey(tauriPage, 'Enter')
    await expect
      .poll(() => dialogText(tauriPage, CONFIRM), { timeout: waitBudget(3000) })
      .toContain('Press Synchronize to run this')
    expect(fs.readFileSync(path.join(dst, 'a.txt'), 'utf8')).toBe('newer on the right!')

    // The explicit button runs it.
    await clickButtonByText(tauriPage, `${CONFIRM} button`, 'Synchronize')
    await expect
      .poll(() => dialogText(tauriPage, DIALOG), { timeout: waitBudget(20_000) })
      .toContain('Synchronization finished')
    await waitForTransferUiToSettle(tauriPage)
    const result = await dialogText(tauriPage, DIALOG)
    expect(result).toContain('.cmdr-sync')
    expect(result).toContain('Show in Cmdr')

    // On disk: the mirror, and the replaced and removed files kept in the target's versions folder.
    expect(fs.readFileSync(path.join(dst, 'a.txt'), 'utf8')).toBe('older on the left')
    expect(fs.readFileSync(path.join(dst, 'b.txt'), 'utf8')).toBe('only on the left')
    expect(fs.existsSync(path.join(dst, 'c.txt'))).toBe(false)
    const kept = filesUnder(path.join(dst, '.cmdr-sync'))
    expect(kept.some((rel) => rel.endsWith('/a.txt'))).toBe(true)
    expect(kept.some((rel) => rel.endsWith('/c.txt'))).toBe(true)

    // Compare again: the synced scope comes back all equal.
    await clickButtonByText(tauriPage, `${DIALOG} button`, 'Compare again')
    await expect
      .poll(() => dialogText(tauriPage, DIALOG), { timeout: waitBudget(10_000) })
      .toContain('The two folders match.')

    await clickButtonByText(tauriPage, `${DIALOG} button`, 'Close')
    await expect.poll(() => dialogText(tauriPage, DIALOG), { timeout: waitBudget(5000) }).toBe('')
  })
})
