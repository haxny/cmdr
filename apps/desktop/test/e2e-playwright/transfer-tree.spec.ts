/**
 * The live transfer tree in the copy progress dialog, end to end: a real
 * throttled local copy, the real backend tree, and the real component.
 *
 * The source is `left/tree-copy/` holding five folders of six files each. Every
 * driver starts files in tree order (folders first, then files, by name), so once
 * N files have landed the work sits in folder `ceil(N / 6)`, every folder before
 * it is done and shut, and every folder after it is waiting and shut. The test
 * reads N off the backend and checks the rendered tree against exactly that.
 *
 * It asserts on a PAUSED copy, never a moving frame: the throttle paces files
 * (one sleep after each, `set_test_throttle`), a pause parks the engine between
 * files, and N is read only once it stops changing. The pause goes in from the
 * page the moment the third folder starts (`pause_operation`, the call the Pause
 * button makes), since a round trip per poll would let a short copy run out first.
 * Parking emits no progress tick, so the tree may still show the frame before the
 * last file; every check below polls until the rows match, and the scroll and the
 * Follow press each pull a fresh window.
 *
 * Unit-level coverage (keyboard, follow, pinned headers) lives beside the
 * component: `src/lib/file-operations/transfer-tree/DETAILS.md` § Tests.
 */

import fs from 'fs'
import path from 'path'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  clickButtonByText,
  dismissAllToasts,
  dispatchMenuCommand,
  drainOperations,
  ensureAppReady,
  flushFileWatcher,
  getFixtureRoot,
  moveCursorToFile,
  TRANSFER_DIALOG,
} from './helpers.js'
import { clickTransferStart } from './conflict-helpers.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

const SOURCE = 'tree-copy'
const FOLDERS = 5
const FILES_PER_FOLDER = 6
/** Per-file pacing: slow enough that a pause lands mid-copy, fast enough to stay a short test. */
const THROTTLE_MS = 60
/** Pause once the third folder has started: two done folders above the work, two waiting below. */
const PAUSE_AFTER = 2 * FILES_PER_FOLDER + 1

const PROGRESS = '[data-dialog-id="transfer-progress"]'
const TREE = `${PROGRESS} [role="tree"]`
const SCROLLER = `${PROGRESS} .transfer-tree .scroller`
const FOLLOW_BUTTON = `${PROGRESS} .transfer-tree .follow button`

function fileName(index: number): string {
  return `file-${String(index).padStart(2, '0')}.txt`
}

function folderName(index: number): string {
  return `folder-${String(index)}`
}

function makeSource(fixtureRoot: string): void {
  for (let folder = 1; folder <= FOLDERS; folder++) {
    const dir = path.join(fixtureRoot, 'left', SOURCE, folderName(folder))
    fs.mkdirSync(dir, { recursive: true })
    for (let file = 1; file <= FILES_PER_FOLDER; file++)
      fs.writeFileSync(path.join(dir, fileName(file)), 'x'.repeat(1024))
  }
}

/** One rendered tree row, as the checks read it: `label state expanded`, or `absent`. */
async function readRow(page: TauriPage, index: number): Promise<string> {
  return page.evaluate<string>(`(function () {
    var row = document.querySelector('${TREE} [role="treeitem"][data-index="${String(index)}"]')
    if (!row) return 'absent'
    var label = (row.getAttribute('aria-label') || '').split(', ')[0]
    return label + ' ' + row.getAttribute('data-state') + ' ' + (row.getAttribute('aria-expanded') || '-')
  })()`)
}

/** `readRow` for each index, one line each, so a poll compares the whole picture. */
async function readRows(page: TauriPage, indices: number[]): Promise<string> {
  const rows: string[] = []
  for (const index of indices) rows.push(`${String(index)}: ${await readRow(page, index)}`)
  return rows.join('\n')
}

function expectRows(rows: [number, string][]): string {
  return rows.map(([index, row]) => `${String(index)}: ${row}`).join('\n')
}

/** Whether the row at `index` is rendered inside the scroller's visible box. */
async function rowInView(page: TauriPage, index: number): Promise<boolean> {
  return page.evaluate<boolean>(`(function () {
    var row = document.querySelector('${TREE} [role="treeitem"][data-index="${String(index)}"]')
    if (!row) return false
    var scroller = document.querySelector('${SCROLLER}').getBoundingClientRect()
    var box = row.getBoundingClientRect()
    return box.top >= scroller.top - 1 && box.bottom <= scroller.bottom + 1
  })()`)
}

/**
 * Waits in the page for the copy to land `count` files, then pauses it at once,
 * and resolves with the operation's id. Polls the backend's own tree, so the
 * threshold is the engine's count, not a rendered frame.
 */
async function pauseAfterFiles(page: TauriPage, count: number): Promise<string> {
  return page.evaluate<string>(`(async function () {
    var deadline = Date.now() + ${String(waitBudget(15000))}
    while (Date.now() < deadline) {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations')
      if (ops.length === 1) {
        var id = ops[0].operationId
        try {
          var view = await window.__TAURI_INTERNALS__.invoke('get_transfer_tree_view', {
            operationId: id,
            request: { expanded: [], collapsed: [], window: { kind: 'at', offset: 0 }, limit: 1 }
          })
          if (view.ready && view.rows.length > 0 && view.rows[0].filesDone >= ${String(count)}) {
            await window.__TAURI_INTERNALS__.invoke('pause_operation', { operationId: id })
            return id
          }
        } catch (e) {}
      }
      await new Promise(function (r) { setTimeout(r, 10) })
    }
    return ''
  })()`)
}

/** Files landed so far, from the backend tree's top row (the copied folder). */
async function filesLanded(page: TauriPage, operationId: string): Promise<number> {
  return page.evaluate<number>(`(async function () {
    var view = await window.__TAURI_INTERNALS__.invoke('get_transfer_tree_view', {
      operationId: ${JSON.stringify(operationId)},
      request: { expanded: [], collapsed: [], window: { kind: 'at', offset: 0 }, limit: 1 }
    })
    return view.ready && view.rows.length > 0 ? view.rows[0].filesDone : -1
  })()`)
}

async function focusedIndex(page: TauriPage): Promise<string> {
  return page.evaluate<string>(`document.activeElement.getAttribute('data-index') || ''`)
}

test.describe('Transfer tree', () => {
  test.describe.configure({ timeout: waitBudget(30000) })

  test.afterEach(async ({ tauriPage }) => {
    // Unthrottle first so the cancel winds down fast, drain, and only then put
    // the tree back: a restore under a live copy deletes its source.
    await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`).catch(() => {})
    await drainOperations(tauriPage).catch(() => {})
    await dismissAllToasts(tauriPage).catch(() => {})
    restoreFixtureTree(getFixtureRoot())
  })

  test('a paused multi-folder copy shows done folders, the folder at work, follows it, and folds by keyboard', async ({
    tauriPage,
  }) => {
    const page = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    recreateFixtures(fixtureRoot)
    makeSource(fixtureRoot)
    await ensureAppReady(page)
    await flushFileWatcher(page)
    await page.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: ${String(THROTTLE_MS)} })`)

    expect(await moveCursorToFile(page, SOURCE)).toBe(true)
    await dispatchMenuCommand(page, 'file.copy')
    await page.waitForSelector(TRANSFER_DIALOG, 5000)
    await clickTransferStart(page)

    const operationId = await pauseAfterFiles(page, PAUSE_AFTER)
    expect(operationId, 'the copy reached the third folder').not.toBe('')
    await page.waitForSelector(`${PROGRESS} [aria-label="Resume this transfer"]`, waitBudget(5000))

    // The engine parks at its next file boundary; N is final once two reads a
    // poll interval apart agree (an unparked copy lands a file every 60 ms).
    let previous = -1
    await expect
      .poll(
        async () => {
          const now = await filesLanded(page, operationId)
          const settled = now === previous
          previous = now
          return settled
        },
        { timeout: waitBudget(5000), intervals: [200] },
      )
      .toBe(true)
    const landed = previous
    expect(landed, 'the pause landed before the copy finished').toBeLessThan(FOLDERS * FILES_PER_FOLDER)
    const workFolder = Math.ceil(landed / FILES_PER_FOLDER)
    const workFile = landed - (workFolder - 1) * FILES_PER_FOLDER
    // Rows: the copied folder, the folders before the work (shut), the work's
    // folder (open), its files, then the folders after it (shut).
    const workRow = workFolder + workFile
    const workFolderState = workFile === FILES_PER_FOLDER ? 'done' : 'active'

    // A wheel is the user's intent to look elsewhere: following pauses and
    // Follow appears. The scroll to the top pulls a fresh window.
    await page.evaluate(`(function () {
      var scroller = document.querySelector('${SCROLLER}')
      scroller.dispatchEvent(new WheelEvent('wheel', { deltaY: -400, bubbles: true }))
      scroller.scrollTop = 0
    })()`)
    await page.waitForSelector(FOLLOW_BUTTON, waitBudget(5000))

    const top: [number, string][] = [[0, `${SOURCE} active true`]]
    for (let folder = 1; folder < workFolder; folder++) top.push([folder, `${folderName(folder)} done false`])
    top.push([workFolder, `${folderName(workFolder)} ${workFolderState} true`])
    top.push([workFolder + 1, `${fileName(1)} done -`])
    await expect
      .poll(
        () =>
          readRows(
            page,
            top.map(([index]) => index),
          ),
        { timeout: waitBudget(5000) },
      )
      .toBe(expectRows(top))

    // Follow brings the last file that started back into view, inside its folder.
    await page.click(FOLLOW_BUTTON)
    await expect
      .poll(async () => (await rowInView(page, workRow)) && !(await page.isVisible(FOLLOW_BUTTON)), {
        timeout: waitBudget(5000),
      })
      .toBeTruthy()
    const around: [number, string][] = [[workRow, `${fileName(workFile)} done -`]]
    if (workFile < FILES_PER_FOLDER) around.push([workRow + 1, `${fileName(workFile + 1)} waiting -`])
    await expect
      .poll(
        () =>
          readRows(
            page,
            around.map(([index]) => index),
          ),
        { timeout: waitBudget(5000) },
      )
      .toBe(expectRows(around))

    // Left from a file steps to its folder; Left again shuts it, and the next
    // folder moves up under it. Right opens it again.
    await page.evaluate(`document.querySelector('${TREE} [role="treeitem"][data-index="${String(workRow)}"]').focus()`)
    await page.keyboard.press('ArrowLeft')
    await expect.poll(() => focusedIndex(page), { timeout: waitBudget(3000) }).toBe(String(workFolder))
    await page.keyboard.press('ArrowLeft')
    const shut: [number, string][] = [[workFolder, `${folderName(workFolder)} ${workFolderState} false`]]
    if (workFolder < FOLDERS) shut.push([workFolder + 1, `${folderName(workFolder + 1)} waiting false`])
    await expect
      .poll(
        () =>
          readRows(
            page,
            shut.map(([index]) => index),
          ),
        { timeout: waitBudget(5000) },
      )
      .toBe(expectRows(shut))
    await page.keyboard.press('ArrowRight')
    const open: [number, string][] = [
      [workFolder, `${folderName(workFolder)} ${workFolderState} true`],
      [workFolder + 1, `${fileName(1)} done -`],
    ]
    await expect
      .poll(
        () =>
          readRows(
            page,
            open.map(([index]) => index),
          ),
        { timeout: waitBudget(5000) },
      )
      .toBe(expectRows(open))

    await clickButtonByText(page, `${PROGRESS} button`, 'Cancel')
    await expect.poll(async () => !(await page.isVisible(PROGRESS)), { timeout: waitBudget(10000) }).toBeTruthy()
  })
})
