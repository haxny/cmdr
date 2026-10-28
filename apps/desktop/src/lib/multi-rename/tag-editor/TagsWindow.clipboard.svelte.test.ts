/**
 * ⌘X / ⌘C / ⌘V in the Tags window: Edit > Cut, Copy and Paste arrive from the native menu (`onTagsClipboard`) and act
 * on the field under the cursor at once: the Album pane's cursor row in Clear's scope, the Tracks pane's cursor cell
 * (or every selected row holding it). A text box and the cover section get the native action back; a dialog takes
 * none of it. A paste is a pending edit, so ⌘Z takes it back.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// The registry reads `isMacOS()` at module load: ⌘ is the Command key here, as in the app.
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)', language: 'en' })
})

import { mount, unmount } from 'svelte'
import type { TagEditorView, TagsClipboardAction } from '#lib/tauri-commands/index.js'

const menu = vi.hoisted(() => ({ clipboard: undefined as ((action: TagsClipboardAction) => void) | undefined }))
const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  editTags: vi.fn(),
  setTagViewScope: vi.fn(),
  undoTagEdit: vi.fn(),
  redoTagEdit: vi.fn(),
  lastTagApply: vi.fn(() => Promise.resolve({ ok: true, value: null })),
  listTagCovers: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  getCoverImage: vi.fn(() => Promise.resolve({ ok: true, value: null })),
  closeTagEditor: vi.fn(() => Promise.resolve()),
  releaseTagsSession: vi.fn(() => Promise.resolve()),
  closeMultiRename: vi.fn(() => Promise.resolve()),
  releaseTagTrackAudio: vi.fn(() => Promise.resolve()),
  onTagsSelectAll: vi.fn(() => Promise.resolve(() => {})),
  onTagsDeselectAll: vi.fn(() => Promise.resolve(() => {})),
  onTagsMenuKey: vi.fn(() => Promise.resolve(() => {})),
  onTagsClipboard: vi.fn((callback: (action: TagsClipboardAction) => void) => {
    menu.clipboard = callback
    return Promise.resolve(() => {})
  }),
  tagsNativeEdit: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
  readClipboardText: vi.fn(),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', async (original) => ({ ...(await original<object>()), ...ipc }))
vi.mock('#lib/child-window-close.js', () => ({ closeSelfWindow: vi.fn(() => Promise.resolve()) }))
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog: vi.fn() }))
vi.mock('#lib/app-status-store.js', () => ({
  TAG_NAME_PATTERNS_MAX: 10,
  loadTagNamePatterns: vi.fn(() => Promise.resolve([])),
  saveTagNamePatterns: vi.fn(() => Promise.resolve()),
  loadTagNumbering: vi.fn(() => Promise.resolve(null)),
  saveTagNumbering: vi.fn(() => Promise.resolve()),
  loadTagWriteKind: vi.fn(() => Promise.resolve(null)),
  saveTagWriteKind: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getRowHeight: () => 20,
  getStripedRows: () => true,
}))

import TagsWindow from './TagsWindow.svelte'
import { settle } from './apply-panel.test-helper'
import { editCursorRow, paneList, toggleTrack } from './tag-cell.test-helper'

const track = (n: string, title: string, readError: TagEditorView['tracks'][number]['readError'] = null) => ({
  path: `/b/${n}.mp3`,
  fileName: `${n}.mp3`,
  track: n,
  title,
  durationMs: 1000,
  changed: false,
  readError,
})

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [
    { field: 'album', value: { same: ['Kniha'] }, choices: [], choicesMore: 0 },
    {
      field: 'artist',
      value: 'multiple',
      choices: [
        { values: ['Jan Novák'], files: 2 },
        { values: ['Eva Malá'], files: 1 },
      ],
      choicesMore: 0,
    },
    { field: 'genre', value: 'empty', choices: [], choicesMore: 0 },
  ],
  tracks: [track('01', 'Prvni'), track('02', 'Druha'), track('03', 'Treti')],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
  canUndoEdit: false,
  canRedoEdit: false,
  scopeCount: null,
  viewSeq: 0,
  sort: null,
}

let cleanup: (() => void) | undefined
beforeEach(() => {
  vi.clearAllMocks()
  ipc.setTagViewScope.mockResolvedValue({ ok: true, value: VIEW })
  menu.clipboard = undefined
  document.body.innerHTML = ''
  ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
  ipc.editTags.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true, canUndoEdit: true } })
  ipc.undoTagEdit.mockResolvedValue({ ok: true, value: VIEW })
  ipc.readClipboardText.mockResolvedValue('Pasted')
})
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountWindow(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const window = mount(TagsWindow, { target, props: { sessionId: 'S' } })
  cleanup = () => {
    void unmount(window)
  }
  await settle()
  return target
}

async function key(name: string, init: KeyboardEventInit = {}): Promise<void> {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...init })
  ;(document.activeElement ?? document.body).dispatchEvent(event)
  await settle()
}

/** Edit > Cut, Copy or Paste picked (or its ⌘ key pressed) while the window is focused. */
async function fromMenu(action: TagsClipboardAction): Promise<void> {
  if (!menu.clipboard) throw new Error('the window is not listening to the menu')
  menu.clipboard(action)
  await settle()
}

describe('⌘C on a field', () => {
  it('copies the Album pane’s cursor row; «multiple values» copies the values without the marker; empty copies ""', async () => {
    await mountWindow()
    await fromMenu('copy')
    await key('ArrowDown')
    await fromMenu('copy')
    await key('ArrowDown')
    await fromMenu('copy')
    expect(ipc.copyToClipboard.mock.calls).toEqual([['Kniha'], ['Jan Novák; Eva Malá'], ['']])
    expect(ipc.editTags).not.toHaveBeenCalled()
    expect(ipc.tagsNativeEdit).not.toHaveBeenCalled()
  })

  it('copies the Tracks pane’s cursor cell: Title, or # after ←', async () => {
    await mountWindow()
    await key('Tab')
    await key('ArrowDown')
    await fromMenu('copy')
    await key('ArrowLeft')
    await fromMenu('copy')
    expect(ipc.copyToClipboard.mock.calls).toEqual([['Druha'], ['02']])
  })

  it('copies in a read-only window too', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { ...VIEW, editable: false } })
    await mountWindow()
    await fromMenu('copy')
    expect(ipc.copyToClipboard).toHaveBeenCalledWith('Kniha')
  })
})

describe('⌘V on a field', () => {
  it('sets the Album row on every file with no track selected, and on the selected tracks alone otherwise', async () => {
    const root = await mountWindow()
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenLastCalledWith('S', [{ kind: 'setAll', field: 'album', values: ['Pasted'] }])
    await toggleTrack(root, '/b/01.mp3')
    await toggleTrack(root, '/b/03.mp3')
    paneList(root, 'Album tags').focus()
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenLastCalledWith('S', [
      { kind: 'setOne', path: '/b/01.mp3', field: 'album', values: ['Pasted'] },
      { kind: 'setOne', path: '/b/03.mp3', field: 'album', values: ['Pasted'] },
    ])
  })

  it('sets the Tracks pane’s cursor row only, unless several rows are selected with the cursor among them', async () => {
    const root = await mountWindow()
    await key('Tab')
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenLastCalledWith('S', [
      { kind: 'setOne', path: '/b/01.mp3', field: 'title', values: ['Pasted'] },
    ])
    // Selected rows without the cursor: only the cursor row.
    await toggleTrack(root, '/b/02.mp3')
    await toggleTrack(root, '/b/03.mp3')
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenLastCalledWith('S', [
      { kind: 'setOne', path: '/b/01.mp3', field: 'title', values: ['Pasted'] },
    ])
    // The cursor on one of the selected rows: all of them, as a spreadsheet fills a selection.
    await key('ArrowDown')
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenLastCalledWith('S', [
      { kind: 'setOne', path: '/b/02.mp3', field: 'title', values: ['Pasted'] },
      { kind: 'setOne', path: '/b/03.mp3', field: 'title', values: ['Pasted'] },
    ])
  })

  it('takes the first line into a one-line field and joins lines with "; " in a multi-value one', async () => {
    ipc.readClipboardText.mockResolvedValue('Jan Novák\nEva Malá\n')
    await mountWindow()
    await fromMenu('paste')
    await key('ArrowDown')
    await fromMenu('paste')
    expect(ipc.editTags.mock.calls).toEqual([
      ['S', [{ kind: 'setAll', field: 'album', values: ['Jan Novák'] }]],
      ['S', [{ kind: 'setAll', field: 'artist', values: ['Jan Novák', 'Eva Malá'] }]],
    ])
  })

  it('pastes a number into # as typing it does', async () => {
    ipc.readClipboardText.mockResolvedValue('7\n')
    await mountWindow()
    await key('Tab')
    await key('ArrowLeft')
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenCalledWith('S', [
      { kind: 'setOne', path: '/b/01.mp3', field: 'track', values: ['7'] },
    ])
  })

  it('says so when the clipboard can’t be read or holds no text, and changes nothing', async () => {
    const root = await mountWindow()
    ipc.readClipboardText.mockRejectedValueOnce(new Error('pasteboard'))
    await fromMenu('paste')
    expect(root.textContent).toContain('Couldn’t read the clipboard.')
    ipc.readClipboardText.mockResolvedValueOnce(null)
    await fromMenu('paste')
    expect(root.textContent).toContain('The clipboard holds no text to paste.')
    expect(ipc.editTags).not.toHaveBeenCalled()
  })

  it('a paste over the field’s limit says so and changes nothing', async () => {
    const root = await mountWindow()
    ipc.readClipboardText.mockResolvedValueOnce('x'.repeat(5000))
    await fromMenu('paste')
    expect(root.textContent).toContain('The clipboard holds too much text for this field, so nothing was pasted.')
    expect(ipc.editTags).not.toHaveBeenCalled()
  })

  it('a blank clipboard says it holds no text, and changes nothing', async () => {
    const root = await mountWindow()
    ipc.readClipboardText.mockResolvedValueOnce(' \n\t')
    await fromMenu('paste')
    expect(root.textContent).toContain('The clipboard holds no text to paste.')
    expect(ipc.editTags).not.toHaveBeenCalled()
  })

  it('never sets an unreadable track', async () => {
    ipc.openTagEditor.mockResolvedValue({
      ok: true,
      value: { ...VIEW, tracks: [track('01', '', 'unreadable'), track('02', 'Druha')] },
    })
    await mountWindow()
    await key('Tab')
    await fromMenu('paste')
    expect(ipc.editTags).not.toHaveBeenCalled()
  })

  it('is a pending edit: ⌘Z takes the paste back', async () => {
    await mountWindow()
    await fromMenu('paste')
    expect(ipc.editTags).toHaveBeenCalledTimes(1)
    await key('z', { metaKey: true, code: 'KeyZ' })
    expect(ipc.undoTagEdit).toHaveBeenCalledWith('S')
  })
})

describe('⌘X on a field', () => {
  it('copies the field, then empties it as Delete does', async () => {
    await mountWindow()
    await key('Tab')
    await fromMenu('cut')
    expect(ipc.copyToClipboard).toHaveBeenCalledWith('Prvni')
    expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setOne', path: '/b/01.mp3', field: 'title', values: [] }])
  })

  it('leaves the field alone when the copy didn’t go through, and says so', async () => {
    ipc.copyToClipboard.mockRejectedValueOnce(new Error('pasteboard'))
    const root = await mountWindow()
    await fromMenu('cut')
    expect(ipc.editTags).not.toHaveBeenCalled()
    expect(root.textContent).toContain('Couldn’t copy to the clipboard.')
  })
})

describe('where ⌘X / ⌘C / ⌘V stay native', () => {
  it.each(['cut', 'copy', 'paste'] as const)('an open value box keeps its own %s', async (action) => {
    const root = await mountWindow()
    await editCursorRow(paneList(root, 'Album tags'))
    await fromMenu(action)
    expect(ipc.tagsNativeEdit).toHaveBeenCalledWith(action)
    expect(ipc.copyToClipboard).not.toHaveBeenCalled()
    expect(ipc.readClipboardText).not.toHaveBeenCalled()
  })

  it('a hand-back that doesn’t go through is logged, never an unhandled rejection', async () => {
    ipc.tagsNativeEdit.mockRejectedValueOnce(new Error('ipc'))
    const root = await mountWindow()
    await editCursorRow(paneList(root, 'Album tags'))
    await fromMenu('paste')
    expect(ipc.tagsNativeEdit).toHaveBeenCalledWith('paste')
  })

  it('the cover section keeps its picture paste', async () => {
    const root = await mountWindow()
    root.querySelector<HTMLElement>('section.cover')?.focus()
    await fromMenu('paste')
    expect(ipc.tagsNativeEdit).toHaveBeenCalledWith('paste')
    expect(ipc.readClipboardText).not.toHaveBeenCalled()
    expect(ipc.editTags).not.toHaveBeenCalled()
  })

  it('a dialog open takes none of it from the fields', async () => {
    await mountWindow()
    await key('F4')
    ;(document.activeElement as HTMLElement | null)?.blur()
    await fromMenu('paste')
    await fromMenu('copy')
    expect(ipc.readClipboardText).not.toHaveBeenCalled()
    expect(ipc.copyToClipboard).not.toHaveBeenCalled()
    expect(ipc.tagsNativeEdit).not.toHaveBeenCalled()
  })
})
