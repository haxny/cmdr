/**
 * Behaviour tests for `SyncFoldersDialog.svelte` over a fake session IPC: it opens on the panes' folders and
 * compares at once, Esc stops a running compare before it closes, a key the cursor row can't take is said instead
 * of dropped, row keys reach the backend, and show-filters never compare again.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import SyncFoldersDialog from './SyncFoldersDialog.svelte'
import * as syncIpcModule from './sync-ipc'
import { ROOTS, STATE, preview, profile, report, settle, type FakeSyncIpc } from './sync-test-fixtures'
import { SyncCommandFailure } from '#lib/tauri-commands/sync.js'
import { runMenuClaim } from '#lib/commands/menu-claims.js'
import { openFileViewer } from '#lib/file-viewer/open-viewer.js'

vi.mock('#lib/file-viewer/open-viewer.js', () => ({ openFileViewer: vi.fn(() => Promise.resolve()) }))

vi.mock('./sync-ipc', async () => {
  const { fakeSyncIpc } = await import('./sync-test-fixtures')
  const fake = fakeSyncIpc()
  return {
    fake,
    syncIpc: fake.ipc,
    readSyncDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
  }
})

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
  quickLookOpen: vi.fn(() => Promise.resolve(true)),
}))

const fake = (syncIpcModule as unknown as { fake: FakeSyncIpc }).fake

let component: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
  vi.clearAllMocks()
})

async function open(): Promise<{ target: HTMLElement; onClose: ReturnType<typeof vi.fn> }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const onClose = vi.fn()
  component = mount(SyncFoldersDialog, { target, props: { roots: ROOTS, onClose } })
  await settle()
  await tick()
  return { target, onClose }
}

async function compared(): Promise<{ target: HTMLElement; onClose: ReturnType<typeof vi.fn> }> {
  const opened = await open()
  fake.finish()
  await settle()
  await tick()
  return opened
}

function grid(target: HTMLElement): HTMLElement {
  const el = target.ownerDocument.querySelector<HTMLElement>('[role="grid"]')
  if (!el) throw new Error('no grid')
  return el
}

function press(el: HTMLElement, init: KeyboardEventInit): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
}

function rowEl(target: HTMLElement, index: number): HTMLElement {
  const el = grid(target).querySelector<HTMLElement>(`.column-list-row.is-data[data-index="${String(index)}"]`)
  if (!el) throw new Error(`no row ${String(index)}`)
  return el
}

describe('SyncFoldersDialog', () => {
  it('opens with both panes as roots and starts a compare', async () => {
    const { target } = await open()
    expect(target.ownerDocument.body.textContent).toContain('Synchronize folders')
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledOnce()
    const request = vi.mocked(fake.ipc.startSyncCompare).mock.calls[0][0]
    expect(request.left.root).toEqual(ROOTS.left.root)
    expect(request.right.root).toEqual(ROOTS.right.root)
    expect(request.options.mode).toBe('bothWays')
    fake.finish()
    await settle()
    await tick()
    expect(rowEl(target, 0).getAttribute('aria-label')).toBe(
      'file-0.txt, newer on the left, will be copied to the right',
    )
    expect(target.ownerDocument.body.textContent).toContain('Photos/2024, folder 1 of 1')
  })

  it('an unmount without a close (a crash under the boundary) still ends the session', async () => {
    await compared()
    if (component) void unmount(component)
    component = null
    await settle()
    expect(fake.ipc.closeSyncSession).toHaveBeenCalledWith('s1')
  })

  it('Esc stops a running compare before closing', async () => {
    const { target, onClose } = await open()
    press(grid(target), { key: 'Escape' })
    await settle()
    expect(fake.ipc.cancelSyncCompare).toHaveBeenCalledWith('s1')
    expect(onClose).not.toHaveBeenCalled()
    fake.finish({ type: 'stopped' })
    await settle()
    await tick()
    press(grid(target), { key: 'Escape' })
    await settle()
    expect(fake.ipc.saveSyncLastOptions).toHaveBeenCalled()
    expect(fake.ipc.closeSyncSession).toHaveBeenCalledWith('s1')
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('says which rows a key left alone and why, as the backend answers it', async () => {
    const { target } = await compared()
    rowEl(target, 0).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    vi.mocked(fake.ipc.setSyncActions).mockResolvedValueOnce({
      state: STATE,
      skipped: [{ reason: { type: 'illegalForMode', action: 'removeRight' }, count: 1 }],
      hidden: 0,
    })
    press(grid(target), { key: 'Backspace', metaKey: true })
    await settle()
    await tick()
    expect(fake.ipc.setSyncActions).toHaveBeenLastCalledWith(
      's1',
      expect.any(Number),
      [0],
      'removeTarget',
      expect.any(Object),
    )
    expect(target.ownerDocument.querySelector('.status')?.textContent).toBe(
      '1 row wasn’t changed: Remove on the right isn’t possible in this mode.',
    )
  })

  it('a row key with no row under the cursor and nothing selected says so', async () => {
    const { target } = await compared()
    press(grid(target), { key: 'l', code: 'KeyL', metaKey: true })
    await settle()
    await tick()
    expect(fake.ipc.setSyncActions).not.toHaveBeenCalled()
    expect(target.ownerDocument.querySelector('.status')?.textContent).toBe(
      'Move the cursor to a file or folder row first.',
    )
  })

  it('⇧-click selects the range from the row clicked before', async () => {
    const { target } = await compared()
    rowEl(target, 0).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    rowEl(target, 4).dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true }))
    await settle()
    await tick()
    press(grid(target), { key: 'n', code: 'KeyN', metaKey: true })
    await settle()
    expect(fake.ipc.setSyncActions).toHaveBeenLastCalledWith(
      's1',
      expect.any(Number),
      [0, 1, 2, 3],
      'skip',
      expect.any(Object),
    )
  })

  it('a menu accelerator for a dialog key runs it once, even when the key also arrives', async () => {
    const { target } = await compared()
    rowEl(target, 0).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    grid(target).focus()
    vi.mocked(fake.ipc.setSyncActions).mockClear()
    // ⌘I is Get Info's accelerator: both roads fire for one press.
    press(grid(target), { key: 'i', code: 'KeyI', metaKey: true })
    expect(runMenuClaim('file.getInfo')).toBe(true)
    await settle()
    expect(fake.ipc.setSyncActions).toHaveBeenCalledOnce()
    expect(fake.ipc.setSyncActions).toHaveBeenLastCalledWith(
      's1',
      expect.any(Number),
      [0],
      'reverse',
      expect.any(Object),
    )
    // ⌘C with focus in a text field declines, so the field copies as usual.
    target.ownerDocument.querySelector<HTMLInputElement>('input')?.focus()
    expect(runMenuClaim('edit.copy')).toBe(false)
  })

  it('⌘PageDown and typed letters jump through the backend’s view', async () => {
    const { target } = await compared()
    vi.mocked(fake.ipc.findSyncRow).mockResolvedValueOnce(3).mockResolvedValueOnce(4)
    press(grid(target), { key: 'PageDown', metaKey: true })
    await settle()
    expect(fake.ipc.findSyncRow).toHaveBeenLastCalledWith('s1', expect.any(Object), { type: 'nextFolder', from: 0 })
    press(grid(target), { key: 'f', code: 'KeyF' })
    await settle()
    expect(fake.ipc.findSyncRow).toHaveBeenLastCalledWith('s1', expect.any(Object), {
      type: 'name',
      from: 4,
      prefix: 'f',
    })
    await tick()
    expect(rowEl(target, 4).getAttribute('aria-selected')).toBe('true')
  })

  it('F3 opens the right side in its own spelling', async () => {
    const { target } = await compared()
    rowEl(target, 4).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    press(grid(target), { key: 'F3', shiftKey: true })
    await settle()
    expect(openFileViewer).toHaveBeenCalledWith('/Volumes/Backup/Photos/photos/2024/file-3.txt', 'root')
  })

  it('sends a row key for the cursor row, and the selection when there is one', async () => {
    const { target } = await compared()
    rowEl(target, 0).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await tick()
    press(grid(target), { key: 'r', code: 'KeyR', metaKey: true })
    await settle()
    expect(fake.ipc.setSyncActions).toHaveBeenLastCalledWith(
      's1',
      expect.any(Number),
      [0],
      'copyLeft',
      expect.any(Object),
    )
    rowEl(target, 3).dispatchEvent(new MouseEvent('click', { bubbles: true, metaKey: true }))
    rowEl(target, 4).dispatchEvent(new MouseEvent('click', { bubbles: true, metaKey: true }))
    await tick()
    press(grid(target), { key: 'n', code: 'KeyN', metaKey: true })
    await settle()
    expect(fake.ipc.setSyncActions).toHaveBeenLastCalledWith(
      's1',
      expect.any(Number),
      [2, 3],
      'skip',
      expect.any(Object),
    )
    expect(rowEl(target, 3).getAttribute('aria-label')).toMatch(/, selected$/)
  })

  it('a show-filter key re-pages without comparing again', async () => {
    const { target } = await compared()
    vi.mocked(fake.ipc.getSyncRows).mockClear()
    press(grid(target), { key: 'E', code: 'KeyE', metaKey: true, shiftKey: true })
    await settle()
    const filters = vi.mocked(fake.ipc.getSyncRows).mock.calls.at(-1)?.[3]
    expect(filters?.equal).toBe(true)
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledOnce()
  })

  it('an option change shows "Compare again" and compares only when asked', async () => {
    const { target } = await compared()
    const doc = target.ownerDocument
    const subfolders = [...doc.querySelectorAll<HTMLElement>('label, [role="checkbox"]')].find((el) =>
      el.textContent.includes('Subfolders'),
    )
    subfolders?.click()
    await settle()
    await tick()
    const again = [...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Compare again')
    expect(again).toBeDefined()
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledOnce()
    again?.click()
    await settle()
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledTimes(2)
  })

  it('⌘↩ opens the confirm; its accept runs, the result shows, and Compare again compares', async () => {
    const { target } = await compared()
    const doc = target.ownerDocument
    vi.mocked(fake.ipc.getSyncConfirm).mockResolvedValueOnce(
      preview({ groups: [{ key: 'copyRight', files: 2, bytes: 20, keep: null }] }),
    )
    press(grid(target), { key: 'Enter', metaKey: true })
    await settle()
    await tick()
    expect(fake.ipc.getSyncConfirm).toHaveBeenCalledWith('s1', expect.any(Object), expect.any(Object))
    const confirm = doc.querySelector<HTMLElement>('[role="dialog"][aria-labelledby="sync-confirm-title"]')
    expect(confirm).not.toBeNull()
    confirm?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await settle()
    await tick()
    expect(fake.ipc.startSyncRun).toHaveBeenCalledOnce()
    expect(doc.body.textContent).toContain('Synchronizing…')
    // While it runs, the grid's menu claims rest: ⌘I does nothing to the rows.
    expect(runMenuClaim('file.getInfo')).toBe(true)
    expect(fake.ipc.setSyncActions).not.toHaveBeenCalled()
    fake.runFinished(report({ done: 2 }))
    await settle()
    await tick()
    expect(doc.body.textContent).toContain('2 rows done')
    const again = [...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Compare again')
    again?.click()
    await settle()
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledTimes(2)
  })

  it('Show in grid goes back to the list with the rows that didn’t run selected', async () => {
    const { target } = await compared()
    const doc = target.ownerDocument
    vi.mocked(fake.ipc.getSyncConfirm).mockResolvedValueOnce(
      preview({ groups: [{ key: 'copyRight', files: 2, bytes: 20, keep: null }] }),
    )
    press(grid(target), { key: 'Enter', metaKey: true })
    await settle()
    await tick()
    ;[...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Synchronize')?.click()
    await settle()
    fake.runFinished(
      report({
        items: [
          { item: 0, result: 'done' },
          { item: 3, result: 'couldNotDo' },
        ],
      }),
    )
    await settle()
    await tick()
    ;[...doc.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Show in grid')?.click()
    await settle()
    await tick()
    expect(doc.querySelector('.status')?.textContent).toBe(
      '1 row that didn’t run is selected. Compare again to see the folders as they are now.',
    )
    expect(rowEl(target, 4).getAttribute('aria-label')).toMatch(/, selected$/)
    expect(fake.ipc.startSyncCompare).toHaveBeenCalledOnce()
  })

  it('releases its menu claims when it unmounts, and a row claim declines outside the grid', async () => {
    const { target } = await compared()
    target.ownerDocument.querySelector<HTMLInputElement>('input')?.focus()
    expect(runMenuClaim('file.getInfo')).toBe(false)
    expect(fake.ipc.setSyncActions).not.toHaveBeenCalled()
    // F2 (File > Rename) is claimed wherever focus is: the dialog's profiles menu takes it.
    expect(runMenuClaim('file.rename')).toBe(true)
    if (component) void unmount(component)
    component = null
    await settle()
    expect(runMenuClaim('file.getInfo')).toBe(false)
    expect(runMenuClaim('file.rename')).toBe(false)
  })

  it('F2 opens the profiles; a profile whose volume is gone says so, and the pane’s folder can stand in', async () => {
    fake.profiles.splice(0, fake.profiles.length, { type: 'valid', profile: profile('p1', 'Photos') })
    const { target } = await compared()
    const doc = target.ownerDocument
    press(grid(target), { key: 'F2' })
    await settle()
    await tick()
    const item = [...doc.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) =>
      el.textContent.includes('Photos'),
    )
    expect(item).toBeDefined()
    vi.mocked(fake.ipc.startSyncCompare).mockRejectedValueOnce(
      new SyncCommandFailure({ type: 'volumeNotConnected', side: 'right', volumeId: 'backup' }),
    )
    item?.click()
    await settle()
    await tick()
    const request = vi.mocked(fake.ipc.startSyncCompare).mock.calls.at(-1)?.[0]
    expect(request?.right.root).toEqual({ volumeId: 'backup', path: '/Volumes/Backup/Photos' })
    expect(request?.profileName).toBe('Photos')
    expect(doc.querySelector('.problem')?.textContent).toContain('right')
    const usePane = [...doc.querySelectorAll('button')].find(
      (b) => b.textContent.trim() === 'Use the right pane’s folder',
    )
    usePane?.click()
    await settle()
    expect(vi.mocked(fake.ipc.startSyncCompare).mock.calls.at(-1)?.[0].right.root).toEqual(ROOTS.right.root)
  })
})
