/**
 * The Tags window's body: the tag content for its session, and the way out. Escape (`tags.close`) closes the window,
 * through the discard guard when edits are unwritten; closing releases the session's hold, ends the tag session and
 * the Multi-Rename session, then asks the backend to close the window (never `getCurrentWindow().close()`).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TagsWindow from './TagsWindow.svelte'
import { editCursorRow, typeInto, press } from './tag-cell.test-helper'

const calls = vi.hoisted(() => [] as string[])
const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  editTags: vi.fn(),
  getTagChanges: vi.fn(),
  closeTagEditor: vi.fn((id: string) => {
    calls.push(`closeTagEditor ${id}`)
    return Promise.resolve()
  }),
  releaseTagsSession: vi.fn((id: string) => {
    calls.push(`release ${id}`)
    return Promise.resolve()
  }),
  closeMultiRename: vi.fn((id: string) => {
    calls.push(`closeMultiRename ${id}`)
    return Promise.resolve()
  }),
  tagTrackAudioUrl: vi.fn(() => Promise.resolve({ ok: false, error: { kind: 'unknownTrack' } })),
  releaseTagTrackAudio: vi.fn(() => Promise.resolve()),
  listTagCovers: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  openTagsInPlayer: vi.fn(() => Promise.resolve({ ok: true, value: null })),
  onTagsSelectAll: vi.fn((callback: () => void) => {
    menuSelectAll.callback = callback
    return Promise.resolve(() => {})
  }),
  onTagsMenuKey: vi.fn(() => Promise.resolve(() => {})),
  onTagsClipboard: vi.fn(() => Promise.resolve(() => {})),
  onTagsDeselectAll: vi.fn((callback: () => void) => {
    menuDeselectAll.callback = callback
    return Promise.resolve(() => {})
  }),
}))
const menuSelectAll = vi.hoisted(() => ({ callback: undefined as (() => void) | undefined }))
const menuDeselectAll = vi.hoisted(() => ({ callback: undefined as (() => void) | undefined }))
const closeSelfWindow = vi.hoisted(() =>
  vi.fn(() => {
    calls.push('closeSelfWindow')
    return Promise.resolve()
  }),
)
const confirmDialog = vi.hoisted(() => vi.fn())

vi.mock('#lib/tauri-commands/index.js', () => ipc)
vi.mock('#lib/child-window-close.js', () => ({ closeSelfWindow }))
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog }))

const VIEW = {
  editable: true,
  readOnlyReason: null,
  shared: [],
  tracks: [
    {
      path: '/m/a.mp3',
      fileName: 'a.mp3',
      track: null,
      title: null,
      durationMs: null,
      changed: false,
      readError: null,
    },
  ],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
  canUndoEdit: false,
  canRedoEdit: false,
  scopeCount: null,
  viewSeq: 0,
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

async function mountWindow(): Promise<ReturnType<typeof mount>> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const window = mount(TagsWindow, { target, props: { sessionId: 'multi-rename-5' } })
  await settle()
  return window
}

function escape(target: EventTarget = document.body): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
  target.dispatchEvent(event)
  return event
}

describe('TagsWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    calls.length = 0
    document.body.innerHTML = ''
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
  })

  it('shows the tag content of its own session', async () => {
    await mountWindow()
    expect(ipc.openTagEditor).toHaveBeenCalledWith('multi-rename-5')
    expect(document.body.textContent).toContain('a.mp3')
  })

  it('window_close_releases_and_closes_the_session: Escape releases, ends both sessions, then closes the window', async () => {
    await mountWindow()
    escape()
    await settle()
    expect(confirmDialog).not.toHaveBeenCalled()
    expect(calls).toEqual([
      'release multi-rename-5',
      'closeTagEditor multi-rename-5',
      'closeMultiRename multi-rename-5',
      'closeSelfWindow',
    ])
  })

  it('with unwritten edits Escape asks first, and Keep editing keeps everything open', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true } })
    confirmDialog.mockResolvedValue(false)
    await mountWindow()
    escape()
    await settle()
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    expect(confirmDialog.mock.calls[0][1]).toBe('Discard tag changes?')
    expect(calls).toEqual([])
  })

  it('Discard closes the window', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true } })
    confirmDialog.mockResolvedValue(true)
    await mountWindow()
    escape()
    await settle()
    expect(calls.at(-1)).toBe('closeSelfWindow')
    expect(ipc.releaseTagsSession).toHaveBeenCalledWith('multi-rename-5')
  })

  it('an Escape something inside already answered (a box undoing its typing, a dialog) never closes the window', async () => {
    await mountWindow()
    const answered = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    answered.preventDefault()
    document.body.dispatchEvent(answered)
    await settle()
    expect(calls).toEqual([])
  })

  it('requestClose (the red button) goes through the same guard', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true } })
    confirmDialog.mockResolvedValue(false)
    const window = await mountWindow()
    ;(window as { requestClose: () => void }).requestClose()
    await settle()
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    expect(calls).toEqual([])
    await unmount(window)
  })

  it('in the Comment editor ⌥⏎ adds a line and ⏎ commits; Esc in an editor closes the editor, not the window', async () => {
    const base = { ...VIEW, shared: [{ field: 'comment', value: 'multiple', choices: [], choicesMore: 0 }] }
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: base })
    ipc.editTags.mockResolvedValue({ ok: true, value: { ...base, dirty: true } })
    await mountWindow()
    const list = document.querySelector<HTMLElement>('[role="listbox"][aria-label="Album tags"]')
    if (!list) throw new Error('no Album pane')
    const box = await editCursorRow(list)
    if (!(box instanceof HTMLTextAreaElement)) throw new Error('no comment textarea')
    await typeInto(box, 'Line one')
    box.setSelectionRange(8, 8)
    const line = await press(box, 'Enter', { altKey: true })
    expect(line.defaultPrevented).toBe(true)
    expect(ipc.editTags).not.toHaveBeenCalled()
    await press(box, 'Enter')
    await settle()
    expect(ipc.editTags).toHaveBeenCalledWith('multi-rename-5', [
      { kind: 'setAll', field: 'comment', values: ['Line one\n'] },
    ])
    expect(ipc.getTagChanges).not.toHaveBeenCalled()

    const again = await editCursorRow(list)
    await press(again, 'Escape')
    await settle()
    expect(document.querySelector('.tag-value-editor')).toBeNull()
    expect(calls).toEqual([])
  })

  it('⌘A from the menu selects every track, or the text of the box being typed in', async () => {
    const first = { ...VIEW.tracks[0], title: 'Alpha' }
    const two = { ...VIEW, tracks: [first, { ...first, path: '/m/b.mp3', fileName: 'b.mp3' }] }
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: two })
    await mountWindow()
    const options = () => [...document.querySelectorAll('[aria-label="Tracks"] [role="option"]')]
    menuSelectAll.callback?.()
    await settle()
    expect(options().map((o) => o.getAttribute('aria-selected'))).toEqual(['true', 'true'])

    const list = document.querySelector<HTMLElement>('[role="listbox"][aria-label="Tracks"]')
    if (!list) throw new Error('no Tracks pane')
    // The header box clears the selection again.
    document.querySelector<HTMLInputElement>('input[aria-label="Select all tracks"]')?.click()
    await settle()
    const box = await editCursorRow(list)
    box.setSelectionRange(0, 0)
    menuSelectAll.callback?.()
    expect([box.selectionStart, box.selectionEnd]).toEqual([0, 'Alpha'.length])
    expect(options().map((o) => o.getAttribute('aria-selected'))).toEqual(['false', 'false'])
  })

  it('⌘⇧A from the menu clears the track selection; ⌘A with focus on the header box still selects the tracks', async () => {
    const first = { ...VIEW.tracks[0], title: 'Alpha' }
    ipc.openTagEditor.mockResolvedValue({
      ok: true,
      value: { ...VIEW, tracks: [first, { ...first, path: '/m/b.mp3', fileName: 'b.mp3' }] },
    })
    await mountWindow()
    const options = () => [...document.querySelectorAll('[aria-label="Tracks"] [role="option"]')]
    // The header box has focus: it holds no text, so ⌘A means the tracks.
    document.querySelector<HTMLInputElement>('input[aria-label="Select all tracks"]')?.focus()
    menuSelectAll.callback?.()
    await settle()
    expect(options().map((o) => o.getAttribute('aria-selected'))).toEqual(['true', 'true'])
    menuDeselectAll.callback?.()
    await settle()
    expect(options().map((o) => o.getAttribute('aria-selected'))).toEqual(['false', 'false'])
  })

  it('a close whose IPC is refused still closes the window', async () => {
    ipc.releaseTagsSession.mockImplementationOnce(() => Promise.reject(new Error('gone')))
    await mountWindow()
    escape()
    await settle()
    expect(calls.at(-1)).toBe('closeSelfWindow')
  })
})
