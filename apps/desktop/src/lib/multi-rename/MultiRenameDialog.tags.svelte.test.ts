/**
 * The tag editor's one foothold in `MultiRenameDialog.svelte`: with the `multiRename.tagEditor` setting off the sheet
 * is upstream's (no Tags… button, no tag IPC at all); with it on, a "Tags…" button asks the main window to open a
 * Tags window over a NEW session of the sheet's files. The sheet itself never holds a tag session, so closing it never
 * ends one. The tag editing (and its discard guard) lives in the window: `tag-editor/TagsWindow.svelte.test.ts`.
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
  reopenForTags: vi.fn(),
  holdTagsSession: vi.fn(),
}))
const openTagsForSession = vi.hoisted(() => vi.fn(() => Promise.resolve()))

vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSetting: (id: string) => (id === 'multiRename.tagEditor' ? flags.tagEditor : false),
}))
vi.mock('#lib/operation-log/recording.js', () => ({ isOperationLogRecording: () => true }))
vi.mock('./tag-editor/open-tags', () => ({ openTagsForSession }))
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getMultiRenameMediaProgress: vi.fn(() => Promise.resolve({ ok: true, value: { read: 0, total: 0, done: true } })),
  onMultiRenameMediaProgress: vi.fn(() => Promise.resolve(() => {})),
  prioritizeMultiRenameMedia: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountSheet(): Promise<ReturnType<typeof mount>> {
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
  return dialog
}

function tagsButton(): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent.trim() === 'Tags…')
}

describe('MultiRenameDialog and the Tags window', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
    flags.tagEditor = false
    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 1, counts: { ready: 0, unchanged: 0, problems: 0, reading: 0 }, rows: [] },
    })
  })

  it('without the setting the sheet is upstream’s: no tab strip, no Tags… button, no tag IPC', async () => {
    await mountSheet()
    expect(document.querySelector('[role="tablist"]')).toBeNull()
    expect(tagsButton()).toBeUndefined()
    expect(document.querySelector('.controls')).not.toBeNull()
    expect(ipc.openTagEditor).not.toHaveBeenCalled()
  })

  it('with the setting the sheet has no tab strip, and a Tags… button', async () => {
    flags.tagEditor = true
    await mountSheet()
    expect(document.querySelector('[role="tablist"]')).toBeNull()
    expect(document.querySelector('[role="tab"]')).toBeNull()
    expect(document.querySelector('.controls')).not.toBeNull()
    expect(tagsButton()).toBeDefined()
    expect(ipc.openTagEditor).not.toHaveBeenCalled()
  })

  it('Tags… asks for a window over the sheet’s files and keeps the sheet open', async () => {
    flags.tagEditor = true
    await mountSheet()
    tagsButton()?.click()
    await settle()
    expect(openTagsForSession).toHaveBeenCalledWith('S')
    expect(document.querySelector('.controls')).not.toBeNull()
  })

  it('closing the sheet never closes a tag session, setting on or off', async () => {
    for (const on of [false, true]) {
      flags.tagEditor = on
      const dialog = await mountSheet()
      await unmount(dialog)
      await settle()
    }
    expect(ipc.closeTagEditor).not.toHaveBeenCalled()
  })
})
