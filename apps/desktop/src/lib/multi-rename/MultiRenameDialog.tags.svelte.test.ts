/**
 * The Tags tab of `MultiRenameDialog.svelte`: with the `multiRename.tagEditor` setting off the sheet is
 * upstream's (no tab strip, no tag IPC at all); with it on, Rename comes first and the tag session opens
 * only when the Tags tab first shows.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { setLastMultiRenameRun } from './last-run.svelte'
import { typeInto, leave, press } from './tag-editor/tag-cell.test-helper'

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
  runTagTextTool: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  undoTagApply: vi.fn(),
  applyMultiRename: vi.fn(),
  rollbackOperation: vi.fn(),
  moveMultiRenameRow: vi.fn(),
}))
const confirmDialog = vi.hoisted(() => vi.fn())
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog }))

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
  ipc.runTagTextTool,
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

async function mountSheet(onClose = vi.fn()): Promise<HTMLElement & { dialog: ReturnType<typeof mount> }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const dialog = mount(MultiRenameDialog, {
    target,
    props: {
      session: { sessionId: 'S', count: 2 },
      nothingSelected: false,
      onApplied: () => {},
      onUndoStarted: () => {},
      onClose,
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
    ipc.openTagEditor.mockResolvedValue({
      ok: true,
      value: {
        editable: true,
        readOnlyReason: null,
        shared: [],
        tracks: ['a', 'b', 'c'].map((n) => ({
          path: `/m/${n}.mp3`,
          fileName: `${n}.mp3`,
          track: null,
          title: null,
          durationMs: null,
          changed: false,
          readError: null,
        })),
        cover: { kind: 'empty' },
        extras: [],
        dirty: false,
        diacritics: [],
      },
    })
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
    // Back and forth asks for the view again (a reorder may have moved the files), never restarting the session.
    tab('Rename').click()
    await settle()
    tab('Tags').click()
    await settle()
    expect(ipc.openTagEditor.mock.calls).toEqual([['S'], ['S']])
    expect(document.querySelector('.controls')).toBeNull()
  })

  it('after a reorder on the Rename tab, the Tags tab lists the files in the new order', async () => {
    const tracksOf = (names: string[]) =>
      names.map((n) => ({
        path: `/m/${n}.mp3`,
        fileName: `${n}.mp3`,
        track: null,
        title: null,
        durationMs: null,
        changed: false,
        readError: null,
      }))
    const view = (names: string[]) => ({
      ok: true,
      value: {
        editable: true,
        readOnlyReason: null,
        shared: [],
        tracks: tracksOf(names),
        cover: { kind: 'empty' },
        extras: [],
        dirty: true,
        diacritics: [],
      },
    })
    const row = (index: number, name: string) => ({
      row: index,
      oldName: `${name}.mp3`,
      newName: `${name}.mp3`,
      status: { type: 'ready' },
      iconId: 'ext:mp3',
      isDirectory: false,
    })
    ipc.previewMultiRename.mockResolvedValue({
      ok: true,
      value: {
        previewId: 3,
        counts: { ready: 3, unchanged: 0, problems: 0, reading: 0 },
        rows: ['a', 'b', 'c'].map((n, i) => row(i, n)),
      },
    })
    ipc.moveMultiRenameRow.mockResolvedValue({ ok: true, value: 1 })
    // The backend session answers in its current order: a, b, c, then b, a, c after the move.
    ipc.openTagEditor.mockResolvedValueOnce(view(['a', 'b', 'c'])).mockResolvedValue(view(['b', 'a', 'c']))
    const listedNames = () =>
      [...document.querySelectorAll('#sheet-panel-tags table.tracks td.file')].map((cell) => cell.textContent.trim())
    flags.tagEditor = true
    await mountSheet()
    tab('Tags').click()
    await settle()
    expect(listedNames()).toEqual(['a.mp3', 'b.mp3', 'c.mp3'])

    tab('Rename').click()
    await settle()
    const grid = document.querySelector<HTMLElement>('[role="grid"]')
    if (!grid) throw new Error('no grid')
    grid.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', shiftKey: true, bubbles: true, cancelable: true }),
    )
    await settle()
    expect(ipc.moveMultiRenameRow).toHaveBeenCalledWith('S', 0, 'down')
    tab('Tags').click()
    await settle()

    expect(listedNames()).toEqual(['b.mp3', 'a.mp3', 'c.mp3'])
    // Unwritten edits live in the backend session: the view comes back with them, never restarted.
    expect(ipc.openTagEditor).toHaveBeenLastCalledWith('S')
    expect(ipc.closeTagEditor).not.toHaveBeenCalled()
  })

  it('arrow keys, Home/End and the ⌘⇧[/⌘⇧] commands switch tabs', async () => {
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
      new KeyboardEvent('keydown', {
        key: '{',
        code: 'BracketLeft',
        metaKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    )
    await settle()
    expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: '}',
        code: 'BracketRight',
        metaKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
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

  describe('the discard guard', () => {
    const DIRTY_VIEW = {
      editable: true,
      readOnlyReason: null,
      shared: [],
      tracks: [],
      cover: { kind: 'empty' },
      extras: [],
      dirty: true,
      diacritics: [],
    }

    function button(text: string): HTMLButtonElement {
      const found = [...document.querySelectorAll<HTMLButtonElement>('button')].find((b) =>
        b.textContent.trim().startsWith(text),
      )
      if (!found) throw new Error(`no ${text} button`)
      return found
    }

    async function mountDirty(onClose = vi.fn()): Promise<void> {
      flags.tagEditor = true
      ipc.openTagEditor.mockResolvedValue({ ok: true, value: DIRTY_VIEW })
      await mountSheet(onClose)
      tab('Tags').click()
      await settle()
    }

    it('close_with_unwritten_changes_asks: Cancel asks "Discard tag changes?" and Keep editing stays open', async () => {
      confirmDialog.mockResolvedValue(false)
      const onClose = vi.fn()
      await mountDirty(onClose)
      button('Cancel').click()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(confirmDialog.mock.calls[0][1]).toBe('Discard tag changes?')
      expect(onClose).not.toHaveBeenCalled()
    })

    it('Discard closes the sheet', async () => {
      confirmDialog.mockResolvedValue(true)
      const onClose = vi.fn()
      await mountDirty(onClose)
      button('Cancel').click()
      await settle()
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('Esc asks too', async () => {
      confirmDialog.mockResolvedValue(false)
      const onClose = vi.fn()
      await mountDirty(onClose)
      document
        .querySelector('.modal-overlay')
        ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(onClose).not.toHaveBeenCalled()
    })

    it('closing with nothing unwritten does not ask', async () => {
      flags.tagEditor = true
      const onClose = vi.fn()
      await mountSheet(onClose)
      tab('Tags').click()
      await settle()
      button('Cancel').click()
      await settle()
      expect(confirmDialog).not.toHaveBeenCalled()
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('rename_with_unwritten_tag_edits_asks_first: declining renames nothing, accepting renames', async () => {
      ipc.previewMultiRename.mockResolvedValue({
        ok: true,
        value: {
          previewId: 1,
          counts: { ready: 1, unchanged: 0, problems: 0, reading: 0 },
          rows: [
            {
              row: 0,
              oldName: 'a.mp3',
              newName: 'b.mp3',
              status: { type: 'ready' },
              iconId: 'ext:mp3',
              isDirectory: false,
            },
          ],
        },
      })
      ipc.applyMultiRename.mockResolvedValue({ ok: true, value: { operationId: 'op', renaming: 1 } })
      confirmDialog.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
      await mountDirty()
      // The footer's Rename button is the Rename tab's; with tag edits unwritten it goes through the guard.
      tab('Rename').click()
      await settle()
      button('Rename 1').click()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(ipc.applyMultiRename).not.toHaveBeenCalled()
      button('Rename 1').click()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(2)
      expect(ipc.applyMultiRename).toHaveBeenCalledTimes(1)
    })

    it('Rename with nothing unwritten does not ask', async () => {
      flags.tagEditor = true
      ipc.previewMultiRename.mockResolvedValue({
        ok: true,
        value: {
          previewId: 1,
          counts: { ready: 1, unchanged: 0, problems: 0, reading: 0 },
          rows: [
            {
              row: 0,
              oldName: 'a.mp3',
              newName: 'b.mp3',
              status: { type: 'ready' },
              iconId: 'ext:mp3',
              isDirectory: false,
            },
          ],
        },
      })
      ipc.applyMultiRename.mockResolvedValue({ ok: true, value: { operationId: 'op', renaming: 1 } })
      await mountSheet()
      button('Rename 1').click()
      await settle()
      expect(confirmDialog).not.toHaveBeenCalled()
      expect(ipc.applyMultiRename).toHaveBeenCalledTimes(1)
    })

    /** A view with one shared row, so a cell can be typed in; the edit's answer is held until `release()`. */
    function holdEdits(): { release: () => void } {
      const base = { ...DIRTY_VIEW, dirty: false, shared: [{ field: 'album', value: 'multiple' }] }
      ipc.openTagEditor.mockResolvedValue({ ok: true, value: base })
      let release: () => void = () => {}
      ipc.editTags.mockImplementation(
        () =>
          new Promise((resolve) => {
            release = () => {
              resolve({ ok: true, value: { ...base, dirty: true } })
            }
          }),
      )
      return {
        release: () => {
          release()
        },
      }
    }

    async function typeThenBlur(): Promise<void> {
      flags.tagEditor = true
      const album = document.querySelector<HTMLInputElement>('#tag-field-album')
      if (!album) throw new Error('no album box')
      await typeInto(album, 'Dracula')
      await leave(album)
    }

    it('Cancel right after typing (edit still pending) asks once the edit has landed', async () => {
      const { release } = holdEdits()
      confirmDialog.mockResolvedValue(false)
      const onClose = vi.fn()
      flags.tagEditor = true
      await mountSheet(onClose)
      tab('Tags').click()
      await settle()
      await typeThenBlur()
      expect(ipc.editTags).toHaveBeenCalledTimes(1)
      button('Cancel').click()
      await settle()
      expect(onClose).not.toHaveBeenCalled()
      expect(confirmDialog).not.toHaveBeenCalled()
      release()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(onClose).not.toHaveBeenCalled()
    })

    it('Rename right after typing (edit still pending) asks once the edit has landed', async () => {
      const { release } = holdEdits()
      ipc.previewMultiRename.mockResolvedValue({
        ok: true,
        value: {
          previewId: 1,
          counts: { ready: 1, unchanged: 0, problems: 0, reading: 0 },
          rows: [
            {
              row: 0,
              oldName: 'a.mp3',
              newName: 'b.mp3',
              status: { type: 'ready' },
              iconId: 'ext:mp3',
              isDirectory: false,
            },
          ],
        },
      })
      ipc.applyMultiRename.mockResolvedValue({ ok: true, value: { operationId: 'op', renaming: 1 } })
      confirmDialog.mockResolvedValue(false)
      flags.tagEditor = true
      await mountSheet()
      tab('Tags').click()
      await settle()
      await typeThenBlur()
      tab('Rename').click()
      await settle()
      button('Rename 1').click()
      await settle()
      release()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(ipc.applyMultiRename).not.toHaveBeenCalled()
    })

    it('Undo rename with unwritten tag edits asks first: declining rolls nothing back, accepting does', async () => {
      setLastMultiRenameRun({ operationId: 'op9', renaming: 3 })
      ipc.rollbackOperation.mockResolvedValue({ inverseOpId: 'inv' })
      confirmDialog.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
      const onUndoStarted = vi.fn()
      flags.tagEditor = true
      ipc.openTagEditor.mockResolvedValue({ ok: true, value: DIRTY_VIEW })
      const target = document.createElement('div')
      document.body.appendChild(target)
      mount(MultiRenameDialog, {
        target,
        props: {
          session: { sessionId: 'S', count: 2 },
          nothingSelected: false,
          onApplied: () => {},
          onUndoStarted,
          onClose: () => {},
        },
      })
      await settle()
      tab('Tags').click()
      await settle()
      button('Undo rename').click()
      await settle()
      expect(confirmDialog).toHaveBeenCalledTimes(1)
      expect(ipc.rollbackOperation).not.toHaveBeenCalled()
      button('Undo rename').click()
      await settle()
      expect(ipc.rollbackOperation).toHaveBeenCalledWith('op9')
      expect(onUndoStarted).toHaveBeenCalledTimes(1)
      setLastMultiRenameRun(null)
    })

    it('on the Tags tab the sheet has no Rename button, and Start (⌘⏎) lists the tag changes instead of renaming', async () => {
      ipc.getTagChanges.mockResolvedValue({ ok: true, value: [] })
      flags.tagEditor = true
      const spy = vi.spyOn(globalThis, 'navigator', 'get')
      spy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
      try {
        await mountDirty()
        const renameButtons = [...document.querySelectorAll('button:not([role="tab"])')].filter((b) =>
          b.textContent.trim().startsWith('Rename'),
        )
        expect(renameButtons).toEqual([])
        document
          .querySelector('.modal-overlay')
          ?.dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }),
          )
        await settle()
        expect(ipc.getTagChanges).toHaveBeenCalledWith('S', 'audiobook')
        expect(ipc.applyMultiRename).not.toHaveBeenCalled()
        expect(confirmDialog).not.toHaveBeenCalled()
      } finally {
        spy.mockRestore()
      }
    })

    it('⌘⇧[ while a Tags box is focused commits its text first', async () => {
      const base = { ...DIRTY_VIEW, dirty: false, shared: [{ field: 'album', value: 'multiple' }] }
      ipc.openTagEditor.mockResolvedValue({ ok: true, value: base })
      ipc.editTags.mockResolvedValue({ ok: true, value: { ...base, dirty: true } })
      flags.tagEditor = true
      const spy = vi.spyOn(globalThis, 'navigator', 'get')
      spy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
      try {
        await mountSheet()
        tab('Tags').click()
        await settle()
        const album = document.querySelector<HTMLInputElement>('#tag-field-album')
        if (!album) throw new Error('no album box')
        await typeInto(album, 'Dracula')
        expect(ipc.editTags).not.toHaveBeenCalled()
        document.body.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: '{',
            code: 'BracketLeft',
            metaKey: true,
            shiftKey: true,
            bubbles: true,
            cancelable: true,
          }),
        )
        await settle()
        expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
        expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setAll', field: 'album', values: ['Dracula'] }])
      } finally {
        spy.mockRestore()
      }
    })

    describe('in a multi-line box (Comment)', () => {
      const base = { ...DIRTY_VIEW, dirty: false, shared: [{ field: 'comment', value: 'multiple' }] }

      async function commentBox(): Promise<HTMLTextAreaElement> {
        ipc.openTagEditor.mockResolvedValue({ ok: true, value: base })
        ipc.editTags.mockResolvedValue({ ok: true, value: { ...base, dirty: true } })
        flags.tagEditor = true
        await mountSheet()
        tab('Tags').click()
        await settle()
        const box = document.querySelector<HTMLTextAreaElement>('textarea#tag-field-comment')
        if (!box) throw new Error('no comment textarea')
        return box
      }

      async function onMac(run: () => Promise<void>): Promise<void> {
        const spy = vi.spyOn(globalThis, 'navigator', 'get')
        spy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
        try {
          await run()
        } finally {
          spy.mockRestore()
        }
      }

      it('⌘⏎ (the registry’s multiRename.rename) commits the text and goes no further; a plain ⏎ is a new line', async () => {
        await onMac(async () => {
          const box = await commentBox()
          await typeInto(box, 'Line one')
          const plain = await press(box, 'Enter')
          expect(plain.defaultPrevented).toBe(false)
          expect(ipc.editTags).not.toHaveBeenCalled()
          await press(box, 'Enter', { metaKey: true })
          await settle()
          expect(ipc.editTags).toHaveBeenCalledWith('S', [{ kind: 'setAll', field: 'comment', values: ['Line one'] }])
          // Claimed by the box: the sheet's Start never lists or writes.
          expect(ipc.getTagChanges).not.toHaveBeenCalled()
        })
      })

      it('typing { and } stays typing; only ⌘⇧[ / ⌘⇧] switch tabs', async () => {
        await onMac(async () => {
          const box = await commentBox()
          for (const [key, code] of [
            ['{', 'BracketLeft'],
            ['}', 'BracketRight'],
            ['[', 'BracketLeft'],
          ] as const) {
            const typed = await press(box, key, { code, shiftKey: key !== '[' })
            expect(typed.defaultPrevented, key).toBe(false)
          }
          await settle()
          expect(tab('Tags').getAttribute('aria-selected')).toBe('true')
          const command = await press(box, '{', { code: 'BracketLeft', metaKey: true, shiftKey: true })
          await settle()
          expect(command.defaultPrevented).toBe(true)
          expect(tab('Rename').getAttribute('aria-selected')).toBe('true')
        })
      })
    })
  })
})
