/**
 * Behavior tests for `MultiRenameDialog.svelte`: every error the backend can
 * answer gets its own words, Enter in a mask starts while Enter in the preset
 * name saves, a placeholder button inserts into the name mask, F2's menu loads,
 * saves, and deletes presets, ⌥⏎ hands Results to the editor and reads it back,
 * ⌥⇧⌫ undoes the last run (plain ⌥⌫ stays a field's word-delete), ⌥⇧↓ opens a
 * field's history and a pick fills the field, and closing remembers the settings.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { setLastMultiRenameRun } from './last-run.svelte'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(),
  saveMultiRenamePreset: vi.fn(),
  deleteMultiRenamePreset: vi.fn(),
  writeMultiRenameNames: vi.fn(),
  readMultiRenameNames: vi.fn(),
  getMultiRenameLastSpec: vi.fn(),
  saveMultiRenameLastSpec: vi.fn(),
  rollbackOperation: vi.fn(),
  getMultiRenameHistory: vi.fn(),
}))
const openFileInEditor = vi.hoisted(() => vi.fn())

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  ...ipc,
}))
vi.mock('$lib/text-editor/open-file-in-editor', () => ({ openFileInEditor }))

const READY = [{ row: 0, oldName: 'Ž.pdf', newName: 'Z.pdf', status: { type: 'ready' } }]

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let mounted: ReturnType<typeof mount> | null = null

async function mountSheet(onApplied = vi.fn(), onUndoStarted = vi.fn()): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(MultiRenameDialog, {
    target,
    props: {
      target: { listingId: 'L', includeHidden: false, rows: null },
      onApplied,
      onUndoStarted,
      onClose: vi.fn(),
    },
  })
  await settle()
  return target
}

function altKey(el: Element, k: string, shiftKey = false): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, altKey: true, shiftKey, bubbles: true, cancelable: true }))
}

function menuItem(label: RegExp): HTMLElement | undefined {
  return [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((el) =>
    label.test(el.textContent.trim()),
  )
}

function inputs(root: HTMLElement): HTMLInputElement[] {
  return [...root.querySelectorAll<HTMLInputElement>('input:not([type="checkbox"])')]
}

function key(el: Element, k: string): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
}

describe('MultiRenameDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: READY })
    ipc.getMultiRenamePresets.mockResolvedValue([{ id: 'p1', name: 'Mine', spec: {} }])
    ipc.saveMultiRenamePreset.mockResolvedValue(undefined)
    ipc.deleteMultiRenamePreset.mockResolvedValue(undefined)
    ipc.getMultiRenameLastSpec.mockResolvedValue(null)
    ipc.getMultiRenameHistory.mockResolvedValue([
      { id: 'h1', field: 'search', value: 'IMG_' },
      { id: 'h2', field: 'nameMask', value: '[N]_[C]' },
      { id: 'h3', field: 'search', value: 'DSC' },
    ])
    ipc.saveMultiRenameLastSpec.mockResolvedValue(undefined)
    setLastMultiRenameRun(null)
  })

  afterEach(async () => {
    if (mounted) await unmount(mounted)
    mounted = null
    document.body.innerHTML = ''
  })

  it.each([
    [{ type: 'spec', error: { type: 'badRegex', detail: 'x' } }],
    [{ type: 'spec', error: { type: 'nameMask', error: { type: 'unclosed', at: 1 } } }],
    [{ type: 'spec', error: { type: 'nameMask', error: { type: 'unknown', placeholder: 'Q' } } }],
    [{ type: 'gone' }],
    [{ type: 'notConnected', volumeId: 'v' }],
    [{ type: 'timedOut' }],
  ])('words a preview error (%o) instead of showing rows', async (error) => {
    ipc.previewMultiRename.mockResolvedValue({ ok: false, error })
    const root = await mountSheet()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it.each([
    [{ type: 'nothingToRename' }],
    [{ type: 'previewOutOfDate' }],
    [{ type: 'readOnly' }],
    [{ type: 'couldntStart', reason: { type: 'busy' } }],
  ])('words a refused start (%o)', async (error) => {
    ipc.applyMultiRename.mockResolvedValue({ ok: false, error })
    const root = await mountSheet()
    key(inputs(root)[0], 'Enter')
    await settle()
    expect(ipc.applyMultiRename).toHaveBeenCalled()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it('starts from Enter in the name mask and hands the operation up', async () => {
    ipc.applyMultiRename.mockResolvedValue({ ok: true, value: { operationId: 'op', renaming: 1 } })
    const onApplied = vi.fn()
    const root = await mountSheet(onApplied)
    key(inputs(root)[0], 'Enter')
    await settle()
    expect(onApplied).toHaveBeenCalledWith({ operationId: 'op', renaming: 1 })
  })

  it('inserts a placeholder into the name mask from its button', async () => {
    const root = await mountSheet()
    const counter = [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === '[C]')
    expect(counter).toBeTruthy()
    counter?.click()
    // The preview reruns after its debounce (`PREVIEW_DELAY_MS`).
    await vi.waitFor(() => {
      const lastSpec = ipc.previewMultiRename.mock.calls.at(-1)?.[3] as { nameMask: string } | undefined
      expect(lastSpec?.nameMask).toContain('[C]')
    })
  })

  it('F2 opens the presets menu; Save as names a preset from Enter, never starting a rename', async () => {
    const root = await mountSheet()
    key(inputs(root)[0], 'F2')
    await settle()
    menuItem(/Save as new preset/)?.click()
    await settle()
    const name = inputs(root).find((i) => i.getAttribute('aria-label') === 'Preset name') as HTMLInputElement
    name.value = 'Fotky'
    name.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    key(name, 'Enter')
    await settle()
    expect(ipc.saveMultiRenamePreset).toHaveBeenCalledWith(expect.objectContaining({ name: 'Fotky' }))
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()
  })

  it('loads a saved preset from the menu, then deletes it from there', async () => {
    const root = await mountSheet()
    key(inputs(root)[0], 'F2')
    await settle()
    menuItem(/^Mine$/)?.click()
    await settle()
    key(inputs(root)[0], 'F2')
    await settle()
    menuItem(/Delete “Mine”/)?.click()
    await settle()
    expect(ipc.deleteMultiRenamePreset).toHaveBeenCalledWith('p1')
  })

  it('⌥⏎ hands the names to the editor, and coming back reads them into the preview', async () => {
    ipc.writeMultiRenameNames.mockResolvedValue({ ok: true, value: '/tmp/names.txt' })
    ipc.readMultiRenameNames.mockResolvedValue({ ok: true, value: [{ oldName: 'Ž.pdf', newName: 'mine.pdf' }] })
    openFileInEditor.mockResolvedValue(true)
    const root = await mountSheet()
    altKey(inputs(root)[0], 'Enter')
    await settle()
    expect(openFileInEditor).toHaveBeenCalledWith('/tmp/names.txt')
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()

    window.dispatchEvent(new Event('focus'))
    await vi.waitFor(() => {
      expect(ipc.previewMultiRename.mock.calls.at(-1)?.[4]).toEqual([{ oldName: 'Ž.pdf', newName: 'mine.pdf' }])
    })
  })

  it('says so when the editor did not open the names', async () => {
    ipc.writeMultiRenameNames.mockResolvedValue({ ok: true, value: '/tmp/names.txt' })
    openFileInEditor.mockResolvedValue(false)
    const root = await mountSheet()
    altKey(inputs(root)[0], 'Enter')
    await settle()
    expect(root.querySelector('[role="alert"]')?.textContent.trim()).toBeTruthy()
  })

  it('⌥⇧⌫ rolls the last run back and hands it up; with no run it does nothing', async () => {
    const onUndoStarted = vi.fn()
    let root = await mountSheet(vi.fn(), onUndoStarted)
    altKey(inputs(root)[0], 'Backspace', true)
    await settle()
    expect(ipc.rollbackOperation).not.toHaveBeenCalled()
    if (mounted) await unmount(mounted)
    document.body.innerHTML = ''

    setLastMultiRenameRun({ operationId: 'op9', renaming: 3 })
    ipc.rollbackOperation.mockResolvedValue({ inverseOpId: 'inv' })
    root = await mountSheet(vi.fn(), onUndoStarted)
    altKey(inputs(root)[0], 'Backspace')
    await settle()
    expect(ipc.rollbackOperation, 'plain ⌥⌫ is the field’s delete-a-word').not.toHaveBeenCalled()
    altKey(inputs(root)[0], 'Backspace', true)
    await settle()
    expect(ipc.rollbackOperation).toHaveBeenCalledWith('op9')
    expect(onUndoStarted).toHaveBeenCalledWith({ operationId: 'op9', renaming: 3 })
  })

  it('⌥⇧↓ in Search lists only what Search held, newest first, and a pick fills the field', async () => {
    const root = await mountSheet()
    const search = inputs(root).find((i) => i.getAttribute('aria-label') === 'Search for') as HTMLInputElement
    altKey(search, 'ArrowDown', true)
    await settle()
    const rows = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].map((el) => el.textContent.trim())
    expect(rows).toEqual(['IMG_', 'DSC'])
    menuItem(/^DSC$/)?.click()
    await vi.waitFor(() => {
      expect((ipc.previewMultiRename.mock.calls.at(-1)?.[3] as { search: string }).search).toBe('DSC')
    })
  })

  it('remembers the settings when it closes', async () => {
    await mountSheet()
    if (mounted) await unmount(mounted)
    mounted = null
    await settle()
    expect(ipc.saveMultiRenameLastSpec).toHaveBeenCalledWith(expect.objectContaining({ nameMask: '[N]' }))
  })
})
