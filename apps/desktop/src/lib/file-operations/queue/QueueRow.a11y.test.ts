import { describe, it, beforeEach, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import QueueRow from './QueueRow.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import type { OperationRow } from './operations-store.svelte'
import type { OperationSnapshot, WriteProgressEvent } from '#lib/ipc/bindings.js'
import { oneCopyingFile } from '../transfer-tree/fixtures/one-copying-file'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'decimal',
}))

// A running transfer's row embeds its live tree, which asks the backend for rows.
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/tauri-commands/index.js')>()),
  getTransferTreeView: vi.fn(() => Promise.resolve(oneCopyingFile)),
}))

function row(
  status: OperationSnapshot['status'],
  opType: OperationSnapshot['operationType'] = 'copy',
  progress: WriteProgressEvent | null = null,
  supportsRollback = false,
): OperationRow {
  return {
    snapshot: {
      operationId: 'op-1',
      operationType: opType,
      status,
      source: '/Users/me/Documents/report.pdf',
      destination: opType === 'delete' || opType === 'trash' ? null : '/Volumes/Backup/report.pdf',
      supportsRollback,
      reverses: null,
      error: null,
      canRetry: false,
    },
    progress,
  }
}

const runningProgress: WriteProgressEvent = {
  operationId: 'op-1',
  operationType: 'copy',
  phase: 'copying',
  currentFile: 'report.pdf',
  filesDone: 1,
  filesTotal: 4,
  bytesDone: 25,
  bytesTotal: 100,
  etaSeconds: 42,
}

beforeEach(() => {
  document.body.innerHTML = ''
})

// QueueRow is a grid `row`, so it's mounted into a `grid` the way the page does,
// to keep the structure valid for axe (a bare `row` is a required-parent violation).
async function mountRow(
  r: OperationRow,
  selected = false,
  isCursor = false,
  treeCollapsible = false,
): Promise<HTMLElement> {
  const list = document.createElement('div')
  list.setAttribute('role', 'grid')
  list.setAttribute('aria-label', 'Operations')
  list.setAttribute('aria-multiselectable', 'true')
  document.body.appendChild(list)
  mount(QueueRow, {
    target: list,
    props: {
      row: r,
      selected,
      isCursor,
      tabbable: isCursor,
      treeCollapsible,
      onToggleSelect: () => {},
      onDismiss: () => {},
    },
  })
  await tick()
  return list
}

describe('QueueRow a11y', () => {
  it('a running copy row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress))
    await expectNoA11yViolations(list)
  })

  it('a paused row has no a11y violations', async () => {
    const list = await mountRow(row('paused', 'copy', runningProgress))
    await expectNoA11yViolations(list)
  })

  it('a queued move row has no a11y violations', async () => {
    const list = await mountRow(row('queued', 'move'))
    await expectNoA11yViolations(list)
  })

  it('a selected delete row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'delete', runningProgress), true)
    await expectNoA11yViolations(list)
  })

  // The cursor row is the grid's Tab stop: focusable, named by its summary and
  // status, and saying whether it's in the "Cancel selected" set.
  it('the row under the cursor has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress), true, true)
    const gridRow = list.querySelector('[role="row"]')
    expect(gridRow?.getAttribute('tabindex')).toBe('0')
    expect(gridRow?.getAttribute('aria-selected')).toBe('true')
    await expectNoA11yViolations(list)
  })

  // The Rollback button is the row's one danger-styled control, and the only
  // one whose accessible name comes from its label rather than an aria-label.
  it('a rollbackable copy row has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress, true))
    await expectNoA11yViolations(list)
  })

  // Show is the row's one control that acts on a WINDOW rather than on the
  // operation, so its spoken name has to say where it goes. WCAG 2.5.3 also
  // wants the visible word inside the accessible name, or "click Show" doesn't
  // reach the control a speech user is looking at.
  it('names the Show button so it says where the operation goes', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress))
    const button = list.querySelector('[aria-label="Show this operation in the main window"]')
    expect(button, 'the running row offers Show').not.toBeNull()
    expect(button?.getAttribute('aria-label')).toContain(button?.textContent.trim())
    await expectNoA11yViolations(list)
  })

  // The live tree sits inside the row as a gridcell (a `row` owns only cells), and on
  // the cursor row it's the next Tab stop.
  it('a cursor row with its live tree has no a11y violations', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress), false, true)
    await vi.waitFor(() => {
      expect(list.querySelector('[role="gridcell"] [role="tree"] [role="treeitem"]')).not.toBeNull()
    })
    await expectNoA11yViolations(list)
  })

  // Two trees in the list: each gets a disclosure button, which must name what it
  // shows and hides, open and shut.
  it('the tree toggle has no a11y violations, open or shut', async () => {
    const list = await mountRow(row('running', 'copy', runningProgress), false, true, true)
    const toggle = list.querySelector<HTMLButtonElement>('[aria-label="Files in this operation"]')
    expect(toggle?.getAttribute('aria-expanded')).toBe('true')
    await vi.waitFor(() => {
      expect(list.querySelector('[role="treeitem"]')).not.toBeNull()
    })
    await expectNoA11yViolations(list)

    toggle?.click()
    await tick()
    expect(toggle?.getAttribute('aria-expanded')).toBe('false')
    await expectNoA11yViolations(list)
  })

  // The failed row is the only one carrying prose, and its explanation is
  // injected as markup by the error pipeline.
  it('a failed row with its reason has no a11y violations', async () => {
    const failed = row('failed', 'copy')
    const list = await mountRow({
      ...failed,
      snapshot: {
        ...failed.snapshot,
        error: { type: 'insufficient_space', required: 1073741824, available: 1024, volumeName: 'Backup' },
      },
    })
    await expectNoA11yViolations(list)
  })
})
