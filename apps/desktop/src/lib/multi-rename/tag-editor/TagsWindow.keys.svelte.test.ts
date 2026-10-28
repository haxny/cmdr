/**
 * The Tags window's function keys: each F-key runs its tag command, the tools open in dialogs over the panes (which
 * stay where they are) and run on the Tracks pane's selection, ⌘S lists the changes in a dialog and writes on
 * confirm, ⌘Z undoes the last write unless a value is being typed, and no F-key fires while a value is typed.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// The registry reads `isMacOS()` at module load: ⌘ is the Command key here, as in the app.
vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)', language: 'en' })
})

import { mount, tick, unmount } from 'svelte'
import type { TagEditorView } from '#lib/tauri-commands/index.js'

const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  editTags: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  undoTagApply: vi.fn(() =>
    Promise.resolve({ ok: true, value: { restored: ['/b/01.mp3'], missing: [], changed: [], failed: [] } }),
  ),
  runTagTool: vi.fn(),
  runTagTextTool: vi.fn(),
  previewTagsFromNames: vi.fn(),
  getCoverImage: vi.fn(),
  listTagCovers: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  listCoverCandidates: vi.fn(),
  closeTagEditor: vi.fn(() => Promise.resolve()),
  releaseTagsSession: vi.fn(() => Promise.resolve()),
  closeMultiRename: vi.fn(() => Promise.resolve()),
  tagTrackAudioUrl: vi.fn(() => Promise.resolve({ ok: false, error: { kind: 'unknownTrack' } })),
  releaseTagTrackAudio: vi.fn(() => Promise.resolve()),
  openTagsInPlayer: vi.fn(() => Promise.resolve({ ok: true, value: null })),
  onTagsSelectAll: vi.fn(() => Promise.resolve(() => {})),
  onTagsDeselectAll: vi.fn(() => Promise.resolve(() => {})),
  onTagsMenuKey: vi.fn((callback: (key: { commandId: string; menuCommandId: string }) => void) => {
    menuKey.callback = callback
    return Promise.resolve(() => {})
  }),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))
const menuKey = vi.hoisted(() => ({
  callback: undefined as ((key: { commandId: string; menuCommandId: string }) => void) | undefined,
}))
const closeSelfWindow = vi.hoisted(() => vi.fn(() => Promise.resolve()))
const confirmDialog = vi.hoisted(() => vi.fn())

vi.mock('#lib/tauri-commands/index.js', async (original) => ({ ...(await original<object>()), ...ipc }))
vi.mock('#lib/child-window-close.js', () => ({ closeSelfWindow }))
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog }))
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
import { setLastTagApply } from './last-tag-apply.svelte'
import { buttonNamed, settle } from './apply-panel.test-helper'
import { editCursorRow, paneList, toggleTrack, typeInto } from './tag-cell.test-helper'

const track = (n: string, title: string) => ({
  path: `/b/${n}.mp3`,
  fileName: `${n}.mp3`,
  track: n,
  title,
  durationMs: 1000,
  changed: false,
  readError: null,
})

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [{ field: 'album', value: { same: ['Kniha'] }, choices: [], choicesMore: 0 }],
  tracks: [track('01', 'Prvni'), track('02', 'Druha'), track('03', 'Treti')],
  cover: { kind: 'empty' },
  extras: ['TXXX:MOOD'],
  dirty: false,
  diacritics: [],
  badDates: 0,
}

let cleanup: (() => void) | undefined
beforeEach(() => {
  vi.clearAllMocks()
  setLastTagApply(null)
  document.body.innerHTML = ''
  ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
  ipc.editTags.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true } })
  ipc.runTagTool.mockResolvedValue({ ok: true, value: VIEW })
  ipc.runTagTextTool.mockResolvedValue({ ok: true, value: VIEW })
  ipc.getCoverImage.mockResolvedValue({ ok: true, value: null })
  ipc.listTagCovers.mockResolvedValue({ ok: true, value: [] })
  ipc.getTagChanges.mockResolvedValue({
    ok: true,
    value: [{ path: '/b/01.mp3', fileName: '01.mp3', fields: ['album'], cover: false, extras: false, removes: [] }],
  })
  ipc.applyTags.mockResolvedValue({
    ok: true,
    value: {
      applyId: 'A1',
      written: ['/b/01.mp3'],
      wouldDrop: [],
      failed: [],
      changedOnDisk: [],
      undoUnavailable: false,
    },
  })
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

/** Presses `key` where focus is (the Album pane on open), as a user would. */
async function key(name: string, init: KeyboardEventInit = {}): Promise<KeyboardEvent> {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...init })
  ;(document.activeElement ?? document.body).dispatchEvent(event)
  await settle()
  return event
}

const dialog = (): HTMLElement | null => document.querySelector('[role="dialog"]')
const dialogTitle = (): string => dialog()?.querySelector('h2, [id$="-title"]')?.textContent.trim() ?? ''

describe('the Tags window’s function keys', () => {
  it.each([
    ['F4', {}, 'Find and replace'],
    ['F5', {}, 'Format a field'],
    ['F6', {}, 'Tags from file names'],
    ['F7', {}, 'Auto-numbering wizard'],
    ['F8', {}, 'Clear a field'],
    ['F4', { shiftKey: true }, 'Change case'],
    ['F5', { shiftKey: true }, 'Clean up'],
    ['F8', { shiftKey: true }, 'Remove extra tags'],
  ] as const)('%s %j opens %s in a dialog over the panes', async (name, init, title) => {
    await mountWindow()
    await key(name, init)
    expect(dialog()).not.toBeNull()
    expect(dialogTitle()).toBe(title)
  })

  it('the panes don’t move when a tool opens: the dialog is an overlay, nothing joins the window’s flow', async () => {
    const root = await mountWindow()
    const tab = root.querySelector<HTMLElement>('.tags-tab')
    if (!tab) throw new Error('no tags tab')
    const flow = () =>
      [...tab.children]
        .filter((el) => !el.querySelector('[role="dialog"]') && el.getAttribute('role') !== 'dialog')
        .map((el) => el.className)
    const before = flow()
    const panes = tab.querySelector('.panes')
    await key('F4')
    expect(dialog()).not.toBeNull()
    expect(flow()).toEqual(before)
    expect(tab.querySelector('.panes')).toBe(panes)
    expect(panes?.contains(dialog())).toBe(false)
  })

  it('Find and replace runs on the Tracks pane’s selection', async () => {
    const root = await mountWindow()
    await toggleTrack(root, '/b/02.mp3')
    await key('F4')
    await typeInto(document.querySelector<HTMLInputElement>('#tag-replace-search') as HTMLInputElement, 'a')
    buttonNamed(dialog() as HTMLElement, 'Replace').click()
    await settle()
    expect(ipc.runTagTextTool).toHaveBeenCalledWith('S', expect.objectContaining({ kind: 'replace' }), ['/b/02.mp3'])
  })

  it('Case changes the field under the Album pane’s cursor, on every track when none is selected', async () => {
    await mountWindow()
    await key('F4', { shiftKey: true })
    expect(dialog()?.textContent).toContain('Changes: Album')
    buttonNamed(dialog() as HTMLElement, 'lower case').click()
    await settle()
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'case', case: 'lower', fields: ['album'] }, null)
    expect(dialog()).toBeNull()
  })

  it('Clean up runs its one-press tools on the selection', async () => {
    const root = await mountWindow()
    await toggleTrack(root, '/b/01.mp3')
    await key('F5', { shiftKey: true })
    buttonNamed(dialog() as HTMLElement, 'Strip track prefix').click()
    await settle()
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'stripTrackPrefix' }, ['/b/01.mp3'])
  })

  it('Clear empties the Tracks pane’s field on the selected tracks only', async () => {
    const root = await mountWindow()
    await key('Tab')
    await toggleTrack(root, '/b/01.mp3')
    await toggleTrack(root, '/b/03.mp3')
    await key('F8')
    expect(dialog()?.textContent).toContain('Title')
    buttonNamed(dialog() as HTMLElement, 'Clear').click()
    await settle()
    expect(ipc.editTags).toHaveBeenCalledWith('S', [
      { kind: 'setOne', path: '/b/01.mp3', field: 'title', values: [] },
      { kind: 'setOne', path: '/b/03.mp3', field: 'title', values: [] },
    ])
  })

  it('Clear with no track selected empties the field on every file', async () => {
    await mountWindow()
    await key('F8')
    buttonNamed(dialog() as HTMLElement, 'Clear').click()
    await settle()
    expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setAll', field: 'album', values: [] }])
  })

  it('Remove extras removes only what is ticked', async () => {
    await mountWindow()
    await key('F8', { shiftKey: true })
    document.querySelector<HTMLInputElement>('[role="dialog"] input[type="checkbox"]')?.click()
    await settle()
    buttonNamed(dialog() as HTMLElement, 'Remove 1 extra tag').click()
    await settle()
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'removeExtras', keys: ['TXXX:MOOD'] }, null)
  })

  it('Esc in a tool dialog closes only the dialog, never the window', async () => {
    await mountWindow()
    await key('F4')
    expect(dialog()).not.toBeNull()
    await key('Escape')
    expect(dialog()).toBeNull()
    expect(closeSelfWindow).not.toHaveBeenCalled()
    expect(ipc.releaseTagsSession).not.toHaveBeenCalled()
  })

  it('F2 with focus nowhere edits in the last pane; F3 plays the Tracks pane’s cursor row; ⇧F3 opens the tracks in the player', async () => {
    await mountWindow()
    ;(document.activeElement as HTMLElement | null)?.blur()
    await key('F2')
    expect(document.querySelector('.tag-value-editor')).not.toBeNull()
    await key('Escape')
    await key('F3')
    expect(ipc.tagTrackAudioUrl).toHaveBeenCalledWith('S', '/b/01.mp3')
    await key('F3', { shiftKey: true })
    expect(ipc.openTagsInPlayer).toHaveBeenCalledWith('S', ['/b/01.mp3', '/b/02.mp3', '/b/03.mp3'])
  })

  it('no F-key fires while a value is being typed', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, 'Dracula')
    for (const name of ['F4', 'F5', 'F6', 'F7', 'F8', 'F3']) await key(name)
    expect(dialog()).toBeNull()
    expect(ipc.tagTrackAudioUrl).not.toHaveBeenCalled()
    expect(document.activeElement).toBe(box)
  })

  it('⌘S lists the changes in a dialog, and writes them on confirm', async () => {
    await mountWindow()
    await key('s', { metaKey: true, code: 'KeyS' })
    expect(dialogTitle()).toBe('Write tags')
    expect(dialog()?.textContent).toContain('01.mp3')
    expect(ipc.applyTags).not.toHaveBeenCalled()
    buttonNamed(dialog() as HTMLElement, 'Write tags').click()
    await settle()
    expect(ipc.applyTags).toHaveBeenCalledTimes(1)
    expect(dialog()?.textContent).toContain('Tags written to 1 file')
  })

  it('⌘S while typing a value sends that value first, then lists', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, 'Dracula')
    await key('s', { metaKey: true, code: 'KeyS' })
    expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setAll', field: 'album', values: ['Dracula'] }])
    expect(ipc.getTagChanges).toHaveBeenCalled()
    expect(dialogTitle()).toBe('Write tags')
  })

  it('⌘Z with no write to undo says so; while a value is typed it is the box’s own undo', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, 'Dracula')
    const inBox = await key('z', { metaKey: true, code: 'KeyZ' })
    expect(inBox.defaultPrevented).toBe(false)
    expect(root.textContent).not.toContain('Nothing to undo')
    await key('Escape')
    await key('z', { metaKey: true, code: 'KeyZ' })
    expect(root.textContent).toContain('There’s no tag write to undo.')
    expect(dialog()).toBeNull()
  })

  it('⌘Z after a write asks, then undoes it, in the Write dialog', async () => {
    await mountWindow()
    await key('s', { metaKey: true, code: 'KeyS' })
    buttonNamed(dialog() as HTMLElement, 'Write tags').click()
    await settle()
    await key('Escape')
    expect(dialog()).toBeNull()
    confirmDialog.mockResolvedValue(true)
    ipc.applyTags.mockClear()
    await key('z', { metaKey: true, code: 'KeyZ' })
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    expect(ipc.undoTagApply).toHaveBeenCalledTimes(1)
    expect(dialogTitle()).toBe('Write tags')
  })

  it('⌘W closes the window through the same guard as Esc', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: { ...VIEW, dirty: true } })
    confirmDialog.mockResolvedValue(false)
    await mountWindow()
    await key('w', { metaKey: true, code: 'KeyW' })
    expect(confirmDialog).toHaveBeenCalledTimes(1)
    expect(closeSelfWindow).not.toHaveBeenCalled()
  })

  it('a key the native menu delivers runs its Tags command (F4 as File > Edit opens Replace)', async () => {
    await mountWindow()
    menuKey.callback?.({ commandId: 'tags.replace', menuCommandId: 'file.edit' })
    await settle()
    expect(dialogTitle()).toBe('Find and replace')
  })

  it('one press seen by both roads (the menu, then the page) acts once', async () => {
    await mountWindow()
    menuKey.callback?.({ commandId: 'tags.play', menuCommandId: 'file.view' })
    await key('F3')
    expect(ipc.tagTrackAudioUrl).toHaveBeenCalledTimes(1)
    // Seen by the page first, then the menu: once too.
    await new Promise((resolve) => setTimeout(resolve, 350))
    await key('F2')
    menuKey.callback?.({ commandId: 'tags.edit', menuCommandId: 'file.rename' })
    await settle()
    expect(document.querySelectorAll('.tag-value-editor')).toHaveLength(1)
  })

  it('a menu key does nothing while a value is typed or a dialog is open', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, 'Dracula')
    menuKey.callback?.({ commandId: 'tags.replace', menuCommandId: 'file.edit' })
    await settle()
    expect(dialog()).toBeNull()
    expect(document.activeElement).toBe(box)
  })

  it('Tab is never taken from the menu: it stays the panes’ and the page’s', async () => {
    const root = await mountWindow()
    menuKey.callback?.({ commandId: 'tags.switchPane', menuCommandId: 'pane.switch' })
    await settle()
    expect(document.activeElement).toBe(paneList(root, 'Album tags'))
  })

  it('Tab in an open tool dialog moves focus within the dialog (its focus trap), never the panes', async () => {
    const root = await mountWindow()
    await key('F8')
    const inDialog = () => dialog()?.contains(document.activeElement) === true
    const tabbables = [...(dialog() as HTMLElement).querySelectorAll<HTMLElement>('button:not([disabled])')]
    const last = tabbables.at(-1)
    if (!last) throw new Error('nothing to tab to in the dialog')
    last.focus()
    await key('Tab')
    expect(inDialog()).toBe(true)
    expect(document.activeElement).not.toBe(last)
    await key('Tab', { shiftKey: true })
    expect(inDialog()).toBe(true)
    expect(document.activeElement).not.toBe(paneList(root, 'Tracks'))
    expect(dialog()).not.toBeNull()
  })

  it('Tab in the value box doesn’t switch the panes', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    const tab = await key('Tab')
    expect(tab.defaultPrevented).toBe(false)
    expect(document.activeElement).not.toBe(paneList(root, 'Tracks'))
    expect(root.querySelector('[data-pane="tracks"] .full-list-container')?.classList.contains('is-focused')).toBe(
      false,
    )
    expect(box.isConnected).toBe(true)
  })

  it('refused text in the value box: a click on Write acts on nothing, and focus settles in the box with no loop', async () => {
    const view = {
      ...VIEW,
      shared: [
        {
          field: 'artist',
          value: 'multiple',
          choices: [
            { values: ['A'], files: 2 },
            { values: ['B'], files: 1 },
          ],
          choicesMore: 0,
        },
      ],
    }
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: view })
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, `${box.value} C`)
    const typed = box.value
    let focusIns = 0
    box.addEventListener('focus', () => {
      focusIns += 1
    })
    const write = [...root.querySelectorAll<HTMLButtonElement>('[role="toolbar"] button')].find((b) =>
      b.textContent.includes('Write'),
    )
    write?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    write?.click()
    for (let i = 0; i < 10; i++) await settle()
    expect(dialog()).toBeNull()
    expect(ipc.getTagChanges).not.toHaveBeenCalled()
    expect(box.isConnected).toBe(true)
    expect(box.value).toBe(typed)
    expect(root.querySelector('.tag-value-hint')).not.toBeNull()
    expect(document.activeElement).toBe(box)
    expect(focusIns).toBeLessThan(2)
  })

  it('a value box with sendable text is sent before a bar command runs', async () => {
    const root = await mountWindow()
    const box = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(box, 'Dracula')
    const replace = [...root.querySelectorAll<HTMLButtonElement>('[role="toolbar"] button')].find((b) =>
      b.textContent.includes('Replace'),
    )
    replace?.click()
    await settle()
    expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setAll', field: 'album', values: ['Dracula'] }])
    expect(dialogTitle()).toBe('Find and replace')
  })

  it('closing the Write dialog on a write that left files unwritten keeps saying so in the status line', async () => {
    ipc.applyTags.mockResolvedValue({
      ok: true,
      value: {
        applyId: 'A1',
        written: [],
        wouldDrop: [],
        failed: [{ path: '/b/01.mp3', error: { type: 'notSupported' } }],
        changedOnDisk: [],
        undoUnavailable: false,
      },
    })
    const root = await mountWindow()
    await key('s', { metaKey: true, code: 'KeyS' })
    buttonNamed(dialog() as HTMLElement, 'Write tags').click()
    await settle()
    await key('Escape')
    expect(dialog()).toBeNull()
    expect(root.querySelector('.status')?.textContent).toContain('Some files weren’t written.')
  })

  it('the bar at the foot runs the same commands', async () => {
    await mountWindow()
    const bar = document.querySelector<HTMLElement>('[role="toolbar"]')
    if (!bar) throw new Error('no function-key bar')
    ;[...bar.querySelectorAll('button')].find((b) => b.textContent.includes('Replace'))?.click()
    await tick()
    await settle()
    expect(dialogTitle()).toBe('Find and replace')
  })
})
