/**
 * The Tags tab of `MultiRenameDialog.svelte`: with the `multiRename.tagEditor` setting off the sheet is
 * upstream's (no tab strip, no tag IPC at all); with it on, Rename comes first and the tag session opens
 * only when the Tags tab first shows.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'

const flags = vi.hoisted(() => ({ tagEditor: false }))
const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenamePreviewRows: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  getMultiRenameRangeSamples: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  renderMultiRenameExamples: vi.fn(() => Promise.resolve([])),
  getMultiRenamePresets: vi.fn(() => Promise.resolve([])),
  getMultiRenameLastSettings: vi.fn(() => Promise.resolve(null)),
  saveMultiRenameLastSettings: vi.fn(() => Promise.resolve()),
  getMultiRenameHistory: vi.fn(() => Promise.resolve([])),
  openTagEditor: vi.fn(),
  closeTagEditor: vi.fn(() => Promise.resolve()),
  editTags: vi.fn(),
  runTagTool: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  undoTagApply: vi.fn(),
}))

vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSetting: (id: string) => (id === 'multiRename.tagEditor' ? flags.tagEditor : false),
}))
vi.mock('#lib/operation-log/recording.js', () => ({ isOperationLogRecording: () => true }))
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getMultiRenameMediaProgress: vi.fn(() => Promise.resolve({ ok: true, value: { read: 0, total: 0, done: true } })),
  onMultiRenameMediaProgress: vi.fn(() => Promise.resolve(() => {})),
  prioritizeMultiRenameMedia: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

const TAG_CALLS = [
  ipc.openTagEditor,
  ipc.closeTagEditor,
  ipc.editTags,
  ipc.runTagTool,
  ipc.getTagChanges,
  ipc.applyTags,
  ipc.undoTagApply,
]

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountSheet(): Promise<HTMLElement & { dialog: ReturnType<typeof mount> }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const dialog = mount(MultiRenameDialog, {
    target,
    props: {
      session: { sessionId: 'S', count: 2 },
      nothingSelected: false,
      onApplied: () => {},
      onUndoStarted: () => {},
      onClose: () => {},
    },
  })
  await settle()
  return Object.assign(target, { dialog })
}

function tab(name: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((el) =>
    el.textContent.trim().startsWith(name),
  )
  if (!found) throw new Error(`no ${name} tab`)
  return found
}

describe('MultiRenameDialog Tags tab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    flags.tagEditor = false
    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 1, counts: { ready: 0, unchanged: 0, problems: 0, reading: 0 }, rows: [] },
    })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { editable: true, tracks: [{}, {}, {}], dirty: false } })
  })

  it('dialog_without_setting_has_no_tabs_and_no_tag_ipc', async () => {
    const root = await mountSheet()
    expect(root.querySelector('[role="tablist"]')).toBeNull()
    expect(document.querySelector('[role="tab"]')).toBeNull()
    expect(document.querySelector('.controls')).not.toBeNull()
    for (const call of TAG_CALLS) expect(call).not.toHaveBeenCalled()
  })

  it('dialog_with_setting_shows_tabs_rename_first', async () => {
    flags.tagEditor = true
    await mountSheet()
    expect(document.querySelectorAll('[role="tablist"]')).toHaveLength(1)
    expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
    expect(tab('Tags').getAttribute('aria-selected')).toBe('false')
    expect(document.querySelector('.controls')).not.toBeNull()
    expect(ipc.openTagEditor).not.toHaveBeenCalled()
  })

  it('tags_tab_opens_tag_session_lazily', async () => {
    flags.tagEditor = true
    await mountSheet()
    expect(ipc.openTagEditor).not.toHaveBeenCalled()
    tab('Tags').click()
    await settle()
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(1)
    expect(ipc.openTagEditor).toHaveBeenCalledWith('S')
    expect(document.querySelector('.controls')).toBeNull()
    expect(document.querySelector('#sheet-panel-tags')?.textContent).toContain('3 tracks')
    // Back and forth does not reopen it.
    tab('Rename').click()
    await settle()
    tab('Tags').click()
    await settle()
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(1)
    expect(document.querySelector('.controls')).toBeNull()
  })

  it('arrow keys, Home/End and the ⌘1/⌘2 commands switch tabs', async () => {
    // `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
    const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
    navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
    try {
      await switchTabs()
    } finally {
      navigatorSpy.mockRestore()
    }
  })

  async function switchTabs(): Promise<void> {
    flags.tagEditor = true
    await mountSheet()
    tab('Rename').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await settle()
    expect(tab('Tags').getAttribute('aria-selected')).toBe('true')
    tab('Tags').dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true }))
    await settle()
    expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
    tab('Rename').dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true }))
    await settle()
    expect(tab('Tags').getAttribute('aria-selected')).toBe('true')
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: '1', code: 'Digit1', metaKey: true, bubbles: true, cancelable: true }),
    )
    await settle()
    expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: '2', code: 'Digit2', metaKey: true, bubbles: true, cancelable: true }),
    )
    await settle()
    expect(tab('Tags').getAttribute('aria-selected')).toBe('true')
  }

  it('the Rename body is a tabpanel the Rename tab controls', async () => {
    flags.tagEditor = true
    await mountSheet()
    const panel = document.querySelector('#sheet-panel-rename')
    expect(panel?.getAttribute('role')).toBe('tabpanel')
    expect(panel?.getAttribute('aria-labelledby')).toBe(tab('Rename').id)
    expect(tab('Rename').getAttribute('aria-controls')).toBe('sheet-panel-rename')
    expect(panel?.querySelector('.controls')).not.toBeNull()
  })

  it('closing the sheet with the setting off never closes a tag session', async () => {
    const root = await mountSheet()
    await unmount(root.dialog)
    await settle()
    expect(ipc.closeTagEditor).not.toHaveBeenCalled()
  })

  it('closing the sheet after the Tags tab showed closes the tag session once', async () => {
    flags.tagEditor = true
    const root = await mountSheet()
    tab('Tags').click()
    await settle()
    await unmount(root.dialog)
    await settle()
    expect(ipc.closeTagEditor).toHaveBeenCalledTimes(1)
    expect(ipc.closeTagEditor).toHaveBeenCalledWith('S')
  })
})
