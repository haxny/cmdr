/**
 * The queue window's "Cancel selected" bookkeeping, driven through the real page.
 *
 * Selection is the one piece of queue state the frontend owns, and it has to
 * stay honest against a row list that outlives its rows: a retained failure
 * STAYS in the list with no checkbox, so a tick the user made while the
 * operation was still running has no way back out by hand. The page prunes it,
 * and these tests mount the page to prove the toolbar follows.
 *
 * The store is the real one; only its two Tauri streams are mocked, so the rows
 * arrive exactly as `operations-changed` delivers them.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import type { OperationSnapshot, QueueMoveOutcome } from '$lib/ipc/bindings'

const cancelOperations = vi.fn((_ids: string[]) => Promise.resolve())
const cancelOperation = vi.fn((_id: string) => Promise.resolve())
const dismissFailedOperation = vi.fn((_id: string) => Promise.resolve())
const requestForegroundOperation = vi.fn((_id: string) => Promise.resolve())
const moveQueuedOperation = vi.fn((_id: string, _direction: string): Promise<QueueMoveOutcome> =>
  Promise.resolve({ kind: 'not_queued' }),
)
/** The ⌘A menu road's listener, as the page registers it. */
let selectAllFromMenu: (() => void) | null = null

vi.mock('$lib/tauri-commands', () => ({
  cancelOperation: (id: string) => cancelOperation(id),
  cancelOperations: (ids: string[]) => cancelOperations(ids),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  dismissAllFailedOperations: vi.fn(() => Promise.resolve()),
  dismissFailedOperation: (id: string) => dismissFailedOperation(id),
  moveQueuedOperation: (id: string, direction: string) => moveQueuedOperation(id, direction),
  onQueueSelectAll: (callback: () => void) => {
    selectAllFromMenu = callback
    return Promise.resolve(() => {})
  },
  requestForegroundOperation: (id: string) => requestForegroundOperation(id),
  pauseAll: vi.fn(() => Promise.resolve()),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeAll: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  // The store's own imports: it subscribes on `init()` and seeds from the list.
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

// The window chrome the page sets up on mount: none of it is under test, and all
// of it would reach for a Tauri window that doesn't exist here.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setFocus: vi.fn(), close: vi.fn() }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('$lib/settings/window-settings', () => ({
  initWindowSettings: vi.fn(() => Promise.resolve()),
  initWindowLanguageSync: vi.fn(() => () => {}),
}))
vi.mock('$lib/accent-color', () => ({
  initAccentColor: vi.fn(() => Promise.resolve()),
  cleanupAccentColor: vi.fn(),
}))
vi.mock('$lib/glass-material', () => ({
  initGlassMaterial: vi.fn(() => Promise.resolve()),
  cleanupGlassMaterial: vi.fn(),
}))
vi.mock('$lib/text-size.svelte', () => ({
  initTextSize: vi.fn(() => Promise.resolve()),
  cleanupTextSize: vi.fn(),
}))
vi.mock('$lib/window-positioning', () => ({ trackOwnRect: vi.fn(() => Promise.resolve(() => {})) }))
// `<Size>` inside the rows reads the reactive settings layer, which needs a live
// settings store the unit environment doesn't have.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ getFileSizeFormat: () => 'decimal' }))

/** The page builds its own store, so the module is wrapped to hand the test the
 *  same instance: snapshots then go in through the reducer the live
 *  `operations-changed` listener calls. */
let store: ReturnType<
  typeof import('$lib/file-operations/queue/operations-store.svelte').createOperationsStore
> | null = null
vi.mock('$lib/file-operations/queue/operations-store.svelte', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/file-operations/queue/operations-store.svelte')>()
  return {
    ...actual,
    createOperationsStore: () => {
      store = actual.createOperationsStore()
      return store
    },
  }
})

import QueuePage from './+page.svelte'

function snapshot(over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: 'op-1',
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Naspolya/Backup',
    supportsRollback: true,
    reverses: null,
    error: null,
    ...over,
  }
}

/** The same operation once it stopped: what the backend retains, in place. */
function failed(operationId: string): OperationSnapshot {
  return snapshot({
    operationId,
    status: 'failed',
    supportsRollback: false,
    reverses: null,
    error: { type: 'io_error', path: '/Users/me/Documents/report.pdf', message: 'disk went away' },
  })
}

let target: HTMLElement
let instance: ReturnType<typeof mount> | undefined

/** Mounts the page and waits out its async `onMount` so the list renders. */
async function renderPage(initial: OperationSnapshot[]): Promise<void> {
  target = document.createElement('div')
  document.body.appendChild(target)
  instance = mount(QueuePage, { target })
  // The list only renders once the page's async `onMount` chain has finished;
  // until then not even the empty state is there.
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('.empty-state')).not.toBeNull()
  })
  applySnapshot(initial)
}

/** Delivers a snapshot exactly as the `operations-changed` listener would. */
function applySnapshot(operations: OperationSnapshot[]): void {
  store?._testApplySnapshot(operations)
  flushSync()
}

function rowFor(operationId: string): HTMLElement | null {
  return target.querySelector(`[data-operation-id="${operationId}"]`)
}

function checkboxFor(operationId: string): HTMLInputElement | null {
  return rowFor(operationId)?.querySelector<HTMLInputElement>('input[type="checkbox"]') ?? null
}

function selectedCountText(): string | null {
  return target.querySelector('.selected-count')?.textContent ?? null
}

/** The toolbar's "Cancel selected", found by its label (it carries none of the
 *  row-level aria labels). */
function cancelSelectedButton(): HTMLButtonElement | null {
  return [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Cancel selected')) ?? null
}

beforeEach(() => {
  document.body.innerHTML = ''
  store = null
  instance = undefined
  cancelOperations.mockClear()
  cancelOperation.mockClear()
  dismissFailedOperation.mockClear()
  requestForegroundOperation.mockClear()
  moveQueuedOperation.mockClear()
  selectAllFromMenu = null
})

afterEach(() => {
  if (instance) void unmount(instance)
})

describe('queue window selection', () => {
  it('counts a checked row and enables "Cancel selected"', async () => {
    await renderPage([snapshot({ operationId: 'op-1' })])

    checkboxFor('op-1')?.click()
    flushSync()

    expect(selectedCountText()).toContain('1')
    expect(cancelSelectedButton()?.disabled).toBe(false)
  })

  it('drops the selection when the checked operation FAILS, so the toolbar stops lying', async () => {
    // The row stays (a retained failure is the whole point) but loses its
    // checkbox, so nothing but this prune can clear the tick. Left in place,
    // "1 selected" stays on screen and "Cancel selected" stays enabled while
    // doing nothing: the backend no-ops an id that isn't live any more.
    await renderPage([snapshot({ operationId: 'op-1' })])
    checkboxFor('op-1')?.click()
    flushSync()
    expect(selectedCountText()).toContain('1')

    applySnapshot([failed('op-1')])

    expect(rowFor('op-1')).not.toBeNull()
    expect(checkboxFor('op-1')).toBeNull()
    expect(selectedCountText()).toBeNull()
    expect(cancelSelectedButton()?.disabled).toBe(true)
  })

  it('keeps a live operation selected when a DIFFERENT one fails', async () => {
    await renderPage([snapshot({ operationId: 'op-1' }), snapshot({ operationId: 'op-2' })])
    checkboxFor('op-2')?.click()
    flushSync()

    applySnapshot([failed('op-1'), snapshot({ operationId: 'op-2' })])

    expect(selectedCountText()).toContain('1')
    expect(checkboxFor('op-2')?.checked).toBe(true)
    expect(cancelSelectedButton()?.disabled).toBe(false)
  })

  it('drops the selection when the operation leaves the list entirely', async () => {
    await renderPage([snapshot({ operationId: 'op-1' })])
    checkboxFor('op-1')?.click()
    flushSync()

    applySnapshot([])

    expect(selectedCountText()).toBeNull()
    expect(cancelSelectedButton()?.disabled).toBe(true)
  })

  it('cancels exactly the checked operations', async () => {
    await renderPage([snapshot({ operationId: 'op-1' }), snapshot({ operationId: 'op-2' })])
    checkboxFor('op-1')?.click()
    flushSync()

    cancelSelectedButton()?.click()

    expect(cancelOperations).toHaveBeenCalledWith(['op-1'])
  })
})

// ── Keyboard (DETAILS § Keyboard) ────────────────────────────────────────────

/** Presses a key where focus is (the bare page when nothing is focused), as the
 *  window's keydown listener receives it. */
function press(init: KeyboardEventInit): void {
  const where = document.activeElement ?? document.body
  where.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
  flushSync()
}

const DOWN = { key: 'ArrowDown', code: 'ArrowDown' }
const UP = { key: 'ArrowUp', code: 'ArrowUp' }

function cursorRowId(): string | null {
  return target.querySelector('.queue-row.is-under-cursor')?.getAttribute('data-operation-id') ?? null
}

function focusedRowId(): string | null {
  return document.activeElement?.closest('.queue-row')?.getAttribute('data-operation-id') ?? null
}

const three = [
  snapshot({ operationId: 'op-1' }),
  snapshot({ operationId: 'op-2', status: 'queued' }),
  snapshot({ operationId: 'op-3', status: 'queued' }),
]

describe('queue window keyboard', () => {
  it('is one Tab stop: a single tabbable row, and every row button out of the Tab order', async () => {
    await renderPage(three)

    expect(target.querySelectorAll('[role="row"][tabindex="0"]')).toHaveLength(1)
    const buttons = [...target.querySelectorAll('[role="row"] button')]
    expect(buttons.length).toBeGreaterThan(0)
    expect(buttons.every((b) => b.getAttribute('tabindex') === '-1')).toBe(true)
  })

  it('↓ from the bare page lands the cursor on the first row and focuses it', async () => {
    await renderPage(three)

    press(DOWN)

    expect(cursorRowId()).toBe('op-1')
    expect(focusedRowId()).toBe('op-1')
  })

  it('↑ / ↓ step, Home / End jump, and the Tab stop follows the cursor', async () => {
    await renderPage(three)
    press(DOWN)
    press(DOWN)
    expect(cursorRowId()).toBe('op-2')
    press({ key: 'End', code: 'End' })
    expect(cursorRowId()).toBe('op-3')
    press(UP)
    expect(cursorRowId()).toBe('op-2')
    press({ key: 'Home', code: 'Home' })
    expect(focusedRowId()).toBe('op-1')
    expect(rowFor('op-1')?.getAttribute('tabindex')).toBe('0')
    expect(rowFor('op-2')?.getAttribute('tabindex')).toBe('-1')
  })

  it('moves the cursor to a row clicked anywhere', async () => {
    await renderPage(three)

    rowFor('op-3')?.querySelector<HTMLElement>('.summary-row')?.click()
    flushSync()

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('Space checks the cursor row for "Cancel selected", and the row says it is selected', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: ' ', code: 'Space' })

    expect(rowFor('op-1')?.getAttribute('aria-selected')).toBe('true')
    expect(checkboxFor('op-1')?.checked).toBe(true)
    expect(selectedCountText()).toContain('1')
  })

  it('⇧↓ extends the selection as the cursor moves', async () => {
    await renderPage(three)
    press(DOWN)

    press({ ...DOWN, shiftKey: true })
    press({ ...DOWN, shiftKey: true })

    expect(selectedCountText()).toContain('3')
    expect(cursorRowId()).toBe('op-3')
  })

  it('⌘A (the menu road) checks every row that can still be canceled', async () => {
    await renderPage([...three, failed('op-4')])

    selectAllFromMenu?.()
    flushSync()

    expect(selectedCountText()).toContain('3')
    expect(rowFor('op-4')?.hasAttribute('aria-selected')).toBe(false)
  })

  it('⌫ cancels the cursor row with no confirmation, like its Cancel button', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: 'Backspace', code: 'Backspace' })

    await vi.waitFor(() => {
      expect(cancelOperation).toHaveBeenCalledWith('op-1')
    })
  })

  it('⌫ dismisses a row that couldn’t finish', async () => {
    await renderPage([failed('op-1')])
    press(DOWN)

    press({ key: 'Delete', code: 'Delete' })

    expect(dismissFailedOperation).toHaveBeenCalledWith('op-1')
  })

  it('Enter shows a running row in the main window, and does nothing on a waiting one', async () => {
    await renderPage(three)
    press(DOWN)
    press({ key: 'Enter', code: 'Enter' })
    expect(requestForegroundOperation).toHaveBeenCalledWith('op-1')

    requestForegroundOperation.mockClear()
    press(DOWN)
    press({ key: 'Enter', code: 'Enter' })
    expect(requestForegroundOperation).not.toHaveBeenCalled()
  })

  it('⌥↑ asks the backend to move the waiting row, and announces where it landed', async () => {
    moveQueuedOperation.mockResolvedValueOnce({ kind: 'moved', position: 1, waiting: 2 })
    await renderPage(three)
    press({ key: 'End', code: 'End' })

    press({ ...UP, altKey: true })

    expect(moveQueuedOperation).toHaveBeenCalledWith('op-3', 'earlier')
    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('[role="status"]')?.textContent).toBe('Moved up. Now 1 of 2 in line.')
    })
  })

  it('keeps the cursor and focus on a reordered row', async () => {
    await renderPage(three)
    press({ key: 'End', code: 'End' })

    applySnapshot([three[0], three[2], three[1]])

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('hands the cursor and focus to the row that slid into place when the cursor row finishes', async () => {
    await renderPage(three)
    press(DOWN)
    press(DOWN)

    applySnapshot([three[0], three[2]])

    expect(cursorRowId()).toBe('op-3')
    expect(focusedRowId()).toBe('op-3')
  })

  it('→ walks into the row’s buttons and ← walks back out to the row', async () => {
    await renderPage(three)
    press(DOWN)

    press({ key: 'ArrowRight', code: 'ArrowRight' })
    expect(document.activeElement?.tagName).toBe('BUTTON')
    expect(focusedRowId()).toBe('op-1')

    press({ key: 'ArrowLeft', code: 'ArrowLeft' })
    expect(document.activeElement).toBe(rowFor('op-1'))
  })
})
