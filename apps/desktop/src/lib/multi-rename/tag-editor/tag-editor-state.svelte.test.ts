/**
 * The Tags window's state: each call keeps the view the backend answered with, a typed error stays as it came,
 * `busy` brackets every call, and Undo reverses exactly the last apply that can be reversed.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'

const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  editTags: vi.fn(),
  sortTagTracks: vi.fn(),
  undoTagEdit: vi.fn(),
  redoTagEdit: vi.fn(),
  runTagTool: vi.fn(),
  runTagTextTool: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  undoTagApply: vi.fn(),
  lastTagApply: vi.fn(),
  closeTagEditor: vi.fn(),
  listCoverCandidates: vi.fn(),
  setCoverFromFile: vi.fn(),
  getCoverImage: vi.fn(),
  listTagCovers: vi.fn(),
  getCoverThumbnail: vi.fn(),
  useCoverForAll: vi.fn(),
  setCoverFromBytes: vi.fn(),
  setCoverFromPath: vi.fn(),
  previewTagsFromNames: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import { createTagEditorState } from './tag-editor-state.svelte'
import { replayLastApply } from './last-apply.test-helper'

const VIEW = { editable: true, tracks: [{}, {}], dirty: false } as unknown as TagEditorView
const EDITED = { editable: true, tracks: [{}, {}], dirty: true } as unknown as TagEditorView
const REQUEST = { kind: 'audiobook', allowFrameLoss: false, skip: [] } as never

function report(over: Partial<TagApplyReport> = {}): TagApplyReport {
  return {
    applyId: 'A1',
    written: ['/a.mp3'],
    wouldDrop: [],
    failed: [],
    changedOnDisk: [],
    undoUnavailable: false,
    ...over,
  }
}

beforeEach(() => {
  vi.resetAllMocks()
  ipc.lastTagApply.mockImplementation(replayLastApply(ipc))
  ipc.closeTagEditor.mockResolvedValue(undefined)
})

describe('createTagEditorState', () => {
  it('open keeps the view it gets', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    expect(state.view).toBeNull()
    await state.open()
    expect(ipc.openTagEditor).toHaveBeenCalledWith('S')
    expect(state.view).toBe(VIEW)
    expect(state.error).toBeNull()
    expect(state.busy).toBe(false)
  })

  it('open keeps the typed error and no view', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    const state = createTagEditorState('S')
    await state.open()
    expect(state.view).toBeNull()
    expect(state.error).toEqual({ kind: 'noSession' })
  })

  it('open twice at once asks the backend once; an open with a view asks again for the current order', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await Promise.all([state.open(), state.open()])
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(1)
    const reordered = { ...VIEW, tracks: [...VIEW.tracks].reverse() }
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: reordered })
    await state.open()
    expect(ipc.openTagEditor.mock.calls).toEqual([['S'], ['S']])
    expect(state.view).toEqual(reordered)
  })

  it('an older view answer that lands after a newer one is dropped', async () => {
    let answerOpen: (res: unknown) => void = () => {}
    ipc.openTagEditor.mockResolvedValueOnce({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    ipc.openTagEditor.mockImplementationOnce(() => new Promise((resolve) => (answerOpen = resolve)))
    const EDITED_VIEW = { ...VIEW, dirty: true }
    ipc.editTags.mockResolvedValueOnce({ ok: true, value: EDITED_VIEW })
    const reopening = state.open()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await state.edit([])
    answerOpen({ ok: true, value: VIEW })
    await reopening
    expect(state.view).toEqual(EDITED_VIEW)
  })

  it('settled() gives up after its limit when a call never answers, and says so', async () => {
    ipc.openTagEditor.mockResolvedValueOnce({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    ipc.editTags.mockImplementation(() => new Promise(() => {}))
    void state.edit([])
    expect(state.busy).toBe(true)
    expect(await state.settled(20)).toBe(false)
    expect(state.busy).toBe(true)
  })

  it('open retries after an error', async () => {
    ipc.openTagEditor
      .mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } })
      .mockResolvedValueOnce({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    await state.open()
    expect(state.view).toBe(VIEW)
    expect(state.error).toBeNull()
  })

  it('busy is on while a call is out', async () => {
    let finish: (value: unknown) => void = () => {}
    ipc.openTagEditor.mockReturnValue(new Promise((resolve) => (finish = resolve)))
    const state = createTagEditorState('S')
    const opening = state.open()
    expect(state.busy).toBe(true)
    finish({ ok: true, value: VIEW })
    await opening
    expect(state.busy).toBe(false)
  })

  it('edit replaces the view, and keeps the old one on an error', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    ipc.editTags.mockResolvedValueOnce({ ok: true, value: EDITED })
    await state.edit([])
    expect(ipc.editTags).toHaveBeenCalledWith('S', [])
    expect(state.view).toBe(EDITED)
    ipc.editTags.mockResolvedValueOnce({ ok: false, error: { kind: 'unknownTrack' } })
    await state.edit([])
    expect(state.view).toBe(EDITED)
    expect(state.error).toEqual({ kind: 'unknownTrack' })
  })

  it('tool replaces the view and reports an error', async () => {
    const state = createTagEditorState('S')
    ipc.runTagTool.mockResolvedValueOnce({ ok: true, value: EDITED })
    await state.tool({ kind: 'stripDiacritics' } as never)
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'stripDiacritics' }, null)
    expect(state.view).toBe(EDITED)
    ipc.runTagTool.mockResolvedValueOnce({ ok: false, error: { kind: 'readOnly' } })
    await state.tool({ kind: 'stripDiacritics' } as never)
    expect(state.error).toEqual({ kind: 'readOnly' })
  })

  it('tool passes the chosen tracks through', async () => {
    const state = createTagEditorState('S')
    ipc.runTagTool.mockResolvedValueOnce({ ok: true, value: EDITED })
    await state.tool({ kind: 'renumber', start: 1 }, ['/a.mp3'])
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'renumber', start: 1 }, ['/a.mp3'])
    expect(state.view).toBe(EDITED)
  })

  it('textTool replaces the view, passes the scope, and keeps a typed error', async () => {
    const state = createTagEditorState('S')
    const tool = { kind: 'format', field: 'title', mask: '[N3-]' } as const
    ipc.runTagTextTool.mockResolvedValueOnce({ ok: true, value: EDITED })
    await state.textTool(tool)
    expect(ipc.runTagTextTool).toHaveBeenCalledWith('S', tool, null)
    expect(state.view).toBe(EDITED)
    ipc.runTagTextTool.mockResolvedValueOnce({ ok: true, value: EDITED })
    await state.textTool(tool, ['/a.mp3'])
    expect(ipc.runTagTextTool).toHaveBeenLastCalledWith('S', tool, ['/a.mp3'])
    ipc.runTagTextTool.mockResolvedValueOnce({ ok: false, error: { kind: 'badRegex' } })
    await state.textTool(tool)
    expect(state.error).toEqual({ kind: 'badRegex' })
    expect(state.view).toBe(EDITED)
  })

  it('sortTracks asks the backend for the order, swaps in the returned view, and keeps a typed error', async () => {
    const state = createTagEditorState('S')
    const sort = { key: 'title', descending: true } as const
    const SORTED = { editable: true, tracks: [{}, {}], dirty: false, sort } as unknown as TagEditorView
    ipc.sortTagTracks.mockResolvedValueOnce({ ok: true, value: SORTED })
    await state.sortTracks(sort)
    expect(ipc.sortTagTracks).toHaveBeenCalledWith('S', sort)
    expect(state.view).toBe(SORTED)
    expect(state.editing).toBe(false)
    ipc.sortTagTracks.mockResolvedValueOnce({ ok: false, error: { kind: 'noSession' } })
    await state.sortTracks({ key: 'fileName', descending: false })
    expect(state.error).toEqual({ kind: 'noSession' })
    expect(state.view).toBe(SORTED)
  })

  it('undoEdit and redoEdit swap in the view the backend answers with, after the edits still out have landed', async () => {
    const state = createTagEditorState('S')
    const UNDONE = { editable: true, tracks: [{}, {}], dirty: false, canRedoEdit: true } as unknown as TagEditorView
    const REDONE = { editable: true, tracks: [{}, {}], dirty: true, canUndoEdit: true } as unknown as TagEditorView
    let finishEdit: (res: unknown) => void = () => {}
    ipc.editTags.mockReturnValueOnce(new Promise((resolve) => (finishEdit = resolve)))
    ipc.undoTagEdit.mockResolvedValueOnce({ ok: true, value: UNDONE })
    ipc.redoTagEdit.mockResolvedValueOnce({ ok: true, value: REDONE })
    const editing = state.edit([])
    const undoing = state.undoEdit()
    await Promise.resolve()
    expect(ipc.undoTagEdit).not.toHaveBeenCalled()
    finishEdit({ ok: true, value: EDITED })
    await editing
    await undoing
    expect(ipc.undoTagEdit).toHaveBeenCalledWith('S')
    expect(state.view).toBe(UNDONE)
    expect(state.editing).toBe(false)
    await state.redoEdit()
    expect(ipc.redoTagEdit).toHaveBeenCalledWith('S')
    expect(state.view).toBe(REDONE)
  })

  it('a failed undoEdit keeps the view and shows the typed error', async () => {
    const state = createTagEditorState('S')
    ipc.openTagEditor.mockResolvedValueOnce({ ok: true, value: VIEW })
    await state.open()
    ipc.undoTagEdit.mockResolvedValueOnce({ ok: false, error: { kind: 'noSession' } })
    await state.undoEdit()
    expect(state.view).toBe(VIEW)
    expect(state.error).toEqual({ kind: 'noSession' })
  })

  it('previewFromNames returns the rows and keeps a typed error', async () => {
    const state = createTagEditorState('S')
    const pattern = { kind: 'format', pattern: '%track% - %title%' } as const
    const rows = [{ path: '/a.mp3', fileName: 'a.mp3', fields: null }]
    ipc.previewTagsFromNames.mockResolvedValueOnce({ ok: true, value: rows })
    expect(await state.previewFromNames(pattern)).toEqual({ ok: true, value: rows })
    expect(ipc.previewTagsFromNames).toHaveBeenLastCalledWith('S', pattern, null)
    const error = { kind: 'badPattern', error: { kind: 'badRegex' } }
    ipc.previewTagsFromNames.mockResolvedValueOnce({ ok: false, error })
    expect(await state.previewFromNames(pattern, ['/a.mp3'])).toEqual({ ok: false, error })
    expect(ipc.previewTagsFromNames).toHaveBeenLastCalledWith('S', pattern, ['/a.mp3'])
    expect(state.error).toEqual(error)
    expect(state.busy).toBe(false)
  })

  it('a good preview clears only the bad-pattern error it owns', async () => {
    const state = createTagEditorState('S')
    const pattern = { kind: 'format', pattern: '%title%' } as const
    ipc.previewTagsFromNames.mockResolvedValue({ ok: true, value: [] })
    ipc.editTags.mockResolvedValueOnce({ ok: false, error: { kind: 'unknownTrack' } })
    await state.edit([])
    await state.previewFromNames(pattern)
    expect(state.error).toEqual({ kind: 'unknownTrack' })
    ipc.previewTagsFromNames.mockResolvedValueOnce({
      ok: false,
      error: { kind: 'badPattern', error: { kind: 'badRegex' } },
    })
    await state.previewFromNames(pattern)
    expect(state.error?.kind).toBe('badPattern')
    await state.previewFromNames(pattern)
    expect(state.error).toBeNull()
  })

  it('changes returns the backend answer; a failure is the Apply panel’s to word, not the tab’s', async () => {
    const state = createTagEditorState('S')
    ipc.getTagChanges.mockResolvedValueOnce({ ok: true, value: [] })
    expect(await state.changes('music')).toEqual({ ok: true, value: [] })
    expect(ipc.getTagChanges).toHaveBeenLastCalledWith('S', 'music')
    ipc.getTagChanges.mockResolvedValueOnce({ ok: false, error: { kind: 'notOpen' } })
    expect(await state.changes('audiobook')).toEqual({ ok: false, error: { kind: 'notOpen' } })
    expect(state.error).toBeNull()
  })

  it('apply returns the report and reloads the view', async () => {
    ipc.applyTags.mockResolvedValue({ ok: true, value: report() })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    const res = await state.apply(REQUEST)
    expect(ipc.applyTags).toHaveBeenCalledWith('S', REQUEST, false)
    expect(res).toEqual({ ok: true, value: report() })
    expect(ipc.openTagEditor).toHaveBeenCalledWith('S')
    expect(state.view).toBe(VIEW)
  })

  it('apply hands back the typed error for the Apply panel to word, and leaves the tab’s error alone', async () => {
    ipc.applyTags.mockResolvedValue({ ok: false, error: { kind: 'noAppDataDir' } })
    const state = createTagEditorState('S')
    const res = await state.apply(REQUEST)
    expect(res).toEqual({ ok: false, error: { kind: 'noAppDataDir' } })
    expect(state.error).toBeNull()
  })

  it('a change list that succeeds leaves an edit’s error standing', async () => {
    ipc.editTags.mockResolvedValueOnce({ ok: false, error: { kind: 'unknownTrack' } })
    ipc.getTagChanges.mockResolvedValueOnce({ ok: true, value: [] })
    const state = createTagEditorState('S')
    await state.edit([])
    await state.changes('audiobook')
    expect(state.error).toEqual({ kind: 'unknownTrack' })
  })

  it('undo reverses the last apply and then has nothing left', async () => {
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A9' }) })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: ['/a.mp3'], missing: [], changed: [], failed: [] },
    })
    const state = createTagEditorState('S')
    await state.apply(REQUEST)
    const res = await state.undo()
    expect(ipc.undoTagApply).toHaveBeenCalledWith('A9', 'S')
    expect(res.ok).toBe(true)
    expect(await state.undo()).toMatchObject({ ok: false, error: { kind: 'nothingToUndo' } })
    expect(ipc.undoTagApply).toHaveBeenCalledTimes(1)
  })

  it('a failed undo stays retryable', async () => {
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A5' }) })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.undoTagApply
      .mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } })
      .mockResolvedValueOnce({ ok: true, value: { restored: [], missing: [], changed: [], failed: [] } })
    const state = createTagEditorState('S')
    await state.apply(REQUEST)
    expect((await state.undo()).ok).toBe(false)
    expect(state.error).toEqual({ kind: 'taskFailed' })
    expect((await state.undo()).ok).toBe(true)
    expect(ipc.undoTagApply).toHaveBeenNthCalledWith(2, 'A5', 'S')
  })

  it('undo with no apply is a typed refusal and no IPC', async () => {
    const state = createTagEditorState('S')
    expect(await state.undo()).toMatchObject({ ok: false, error: { kind: 'nothingToUndo' } })
    expect(ipc.undoTagApply).not.toHaveBeenCalled()
  })

  it('undo after an apply with no applyId or no undo is refused', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    ipc.applyTags.mockResolvedValueOnce({ ok: true, value: report({ applyId: null, written: [] }) })
    await state.apply(REQUEST)
    expect((await state.undo()).ok).toBe(false)
    ipc.applyTags.mockResolvedValueOnce({ ok: true, value: report({ undoUnavailable: true }) })
    await state.apply(REQUEST)
    expect(await state.undo()).toMatchObject({ ok: false, error: { kind: 'undoUnavailable' } })
    expect(ipc.undoTagApply).not.toHaveBeenCalled()
  })

  it('a "Write anyway" apply continues the write: Undo reverses both, the last one first', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A1', written: ['/a.mp3', '/b.mp3'] }) })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A2', written: ['/c.mp3'] }) })
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: ['/x.mp3'], missing: [], changed: [], failed: [] },
    })
    const state = createTagEditorState('S')
    const first = await state.apply(REQUEST)
    if (!first.ok) throw new Error('the first apply')
    await state.apply(REQUEST, { after: [first.value] })
    expect(state.undoable).toEqual({
      applies: [
        { applyId: 'A1', written: 2 },
        { applyId: 'A2', written: 1 },
      ],
      partial: false,
    })
    const res = await state.undo()
    expect(ipc.undoTagApply.mock.calls).toEqual([
      ['A2', 'S'],
      ['A1', 'S'],
    ])
    expect(res).toEqual({ ok: true, value: { restored: ['/x.mp3', '/x.mp3'], missing: [], changed: [], failed: [] } })
    expect(state.undoable).toBeNull()
  })

  it('a new write replaces the last one, so Undo never reaches an older write', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A1' }) })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A2' }) })
    const state = createTagEditorState('S')
    await state.apply(REQUEST)
    await state.apply(REQUEST)
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A2', written: 1 }], partial: false })
  })

  it('an undo that fails halfway keeps what is left undoable, and reloads what it restored', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    // The backend holds a two-part write; after the undo of A2 (A1's failed) it holds A1 alone.
    ipc.lastTagApply
      .mockResolvedValueOnce({
        ok: true,
        value: {
          applies: [
            { applyId: 'A1', written: 3 },
            { applyId: 'A2', written: 1 },
          ],
          partial: false,
        },
      })
      .mockResolvedValue({ ok: true, value: { applies: [{ applyId: 'A1', written: 3 }], partial: false } })
    ipc.undoTagApply
      .mockResolvedValueOnce({ ok: true, value: { restored: ['/b.mp3'], missing: [], changed: [], failed: [] } })
      .mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } })
    const state = createTagEditorState('S')
    await state.open()
    const res = await state.undo()
    // What it restored before it stopped comes with the error, and the retry counts only what's left.
    expect(res).toEqual({
      ok: false,
      error: { kind: 'taskFailed' },
      restored: { restored: ['/b.mp3'], missing: [], changed: [], failed: [] },
    })
    expect(state.error).toEqual({ kind: 'taskFailed' })
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A1', written: 3 }], partial: false })
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(2)
  })

  it('an apply whose files could not all be restored stays undoable', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A1', written: ['/a.mp3', '/b.mp3'] }) })
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: ['/a.mp3'], missing: [], changed: [], failed: [['/b.mp3', { type: 'notSupported' }]] },
    })
    const state = createTagEditorState('S')
    await state.apply(REQUEST)
    expect((await state.undo()).ok).toBe(true)
    // Only the file it couldn't restore is left for a retry.
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A1', written: 1 }], partial: false })
  })

  it('a write that wrote nothing keeps the last write undoable', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A1' }) })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: null, written: [] }) })
      .mockResolvedValueOnce({
        ok: true,
        value: report({ applyId: null, written: [], wouldDrop: [{ path: '/b.mp3', items: ['TSOP'] }] }),
      })
    const state = createTagEditorState('S')
    await state.apply(REQUEST)
    await state.apply(REQUEST)
    await state.apply(REQUEST)
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A1', written: 1 }], partial: false })
  })

  it('a write whose first part kept no undo marks the write partial', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A1', undoUnavailable: true }) })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A2', written: ['/c.mp3'] }) })
    const state = createTagEditorState('S')
    const first = await state.apply(REQUEST)
    if (!first.ok) throw new Error('the first apply')
    await state.apply(REQUEST, { after: [first.value] })
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A2', written: 1 }], partial: true })
  })

  it('a "Write anyway" after a first apply that wrote nothing starts its own write, apart from the last press', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const wouldDropOnly = report({ applyId: null, written: [], wouldDrop: [{ path: '/b.mp3', items: ['TSOP'] }] })
    ipc.applyTags
      // Press 1 writes, and its first part kept no undo for one file: partial.
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A0', undoUnavailable: true }) })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A1', written: ['/c.mp3'] }) })
      // Press 2: the first apply writes nothing, only would-drop; "Write anyway" writes A2.
      .mockResolvedValueOnce({ ok: true, value: wouldDropOnly })
      .mockResolvedValueOnce({ ok: true, value: report({ applyId: 'A2', written: ['/b.mp3'] }) })
    const state = createTagEditorState('S')
    const press1 = await state.apply(REQUEST)
    if (!press1.ok) throw new Error('press 1')
    await state.apply(REQUEST, { after: [press1.value] })
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A1', written: 1 }], partial: true })
    const press2 = await state.apply(REQUEST)
    if (!press2.ok) throw new Error('press 2')
    await state.apply(REQUEST, { after: [press2.value] })
    expect(state.undoable).toEqual({ applies: [{ applyId: 'A2', written: 1 }], partial: false })
  })

  it('the write outlives its window: a window opened later over the same folder can undo it (⌘Z)', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A7' }) })
    ipc.undoTagApply.mockResolvedValue({ ok: true, value: { restored: [], missing: [], changed: [], failed: [] } })
    const first = createTagEditorState('S')
    await first.open()
    await first.apply(REQUEST)
    await first.close()
    expect(first.undoable).toBeNull()

    const later = createTagEditorState('T')
    expect(later.undoable).toBeNull()
    await later.open()
    // The window mirrors what the backend keeps for the folder; it asked with its own session.
    expect(ipc.lastTagApply).toHaveBeenLastCalledWith('T')
    expect(later.undoable).toEqual({ applies: [{ applyId: 'A7', written: 1 }], partial: false })
    expect((await later.undo()).ok).toBe(true)
    expect(ipc.undoTagApply).toHaveBeenCalledWith('A7', 'T')
    expect(later.undoable).toBeNull()
  })

  it('a backend that can’t say what the last write was leaves Undo with nothing, and is logged', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.lastTagApply.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    const state = createTagEditorState('S')
    await state.open()
    expect(state.undoable).toBeNull()
    expect(warn).toHaveBeenCalledWith("couldn't ask for the last tag write: {kind}", { kind: 'noSession' })
  })

  it('writing is on while a write or an undo is out, and editing only while an edit is', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    let finish: (value: unknown) => void = () => {}
    ipc.applyTags.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve
      }),
    )
    const state = createTagEditorState('S')
    await state.open()
    expect(state.writing).toBe(false)
    const applying = state.apply(REQUEST)
    expect(state.writing).toBe(true)
    expect(state.editing).toBe(false)
    finish({ ok: true, value: report() })
    await applying
    expect(state.writing).toBe(false)
    ipc.editTags.mockReturnValue(new Promise(() => {}))
    void state.edit([])
    expect(state.editing).toBe(true)
    expect(state.writing).toBe(false)
  })

  it('reload reads every file again', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.reload()
    expect(ipc.openTagEditor).toHaveBeenCalledWith('S', true)
    expect(state.view).toBe(VIEW)
  })

  it('close ends the backend session and drops the view', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    await state.close()
    expect(ipc.closeTagEditor).toHaveBeenCalledWith('S')
    expect(state.view).toBeNull()
  })

  it('an open that lands after close does not bring the view back', async () => {
    let finish: (value: unknown) => void = () => {}
    ipc.openTagEditor.mockReturnValue(new Promise((resolve) => (finish = resolve)))
    const state = createTagEditorState('S')
    const opening = state.open()
    await state.close()
    finish({ ok: true, value: VIEW })
    await opening
    expect(state.view).toBeNull()
  })

  it('a close that fails is logged, not thrown', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.closeTagEditor.mockRejectedValue(new Error('gone'))
    const state = createTagEditorState('S')
    await state.open()
    await expect(state.close()).resolves.toBeUndefined()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('close before open does not touch the backend', async () => {
    const state = createTagEditorState('S')
    await state.close()
    expect(ipc.closeTagEditor).not.toHaveBeenCalled()
  })

  it('coverCandidates lists the folder images, and a typed error stays', async () => {
    ipc.listCoverCandidates.mockResolvedValue({ ok: true, value: [{ fileName: 'a.jpg', size: 3 }] })
    const state = createTagEditorState('S')
    expect(await state.coverCandidates()).toEqual({ ok: true, value: [{ fileName: 'a.jpg', size: 3 }] })
    expect(ipc.listCoverCandidates).toHaveBeenCalledWith('S')
    ipc.listCoverCandidates.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    expect((await state.coverCandidates()).ok).toBe(false)
    expect(state.error).toEqual({ kind: 'noSession' })
  })

  it('coverFromFile replaces the view, and keeps the old one on a typed error', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.setCoverFromFile.mockResolvedValue({ ok: true, value: EDITED })
    const state = createTagEditorState('S')
    await state.open()
    await state.coverFromFile('a.jpg')
    expect(ipc.setCoverFromFile).toHaveBeenCalledWith('S', 'a.jpg')
    expect(state.view).toBe(EDITED)
    ipc.setCoverFromFile.mockResolvedValue({ ok: false, error: { kind: 'coverTooLarge' } })
    await state.coverFromFile('big.jpg')
    expect(state.view).toBe(EDITED)
    expect(state.error).toEqual({ kind: 'coverTooLarge' })
  })

  it('settled waits for every call now out, busy included', async () => {
    let answer: (res: unknown) => void = () => {}
    ipc.editTags.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve
        }),
    )
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await state.open()
    const edit = state.edit([])
    expect(state.busy).toBe(true)
    let settled = false
    void state.settled().then(() => {
      settled = true
    })
    await Promise.resolve()
    expect(settled).toBe(false)
    answer({ ok: true, value: EDITED })
    await edit
    await state.settled()
    expect(settled).toBe(true)
    expect(state.busy).toBe(false)
  })

  it('coverImage returns the data URL, or null (logged by kind) without touching busy or the error', async () => {
    const state = createTagEditorState('S')
    ipc.getCoverImage.mockResolvedValue({ ok: true, value: 'data:image/png;base64,iVBORw0KGgo=' })
    expect(await state.coverImage()).toBe('data:image/png;base64,iVBORw0KGgo=')
    ipc.getCoverImage.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    expect(await state.coverImage()).toBeNull()
    expect(state.error).toBeNull()
    expect(warn).toHaveBeenCalled()
  })

  it('coverImage asks for the cover a choice names', async () => {
    const state = createTagEditorState('S')
    ipc.getCoverImage.mockResolvedValue({ ok: true, value: null })
    await state.coverImage('00ff')
    expect(ipc.getCoverImage).toHaveBeenCalledWith('S', '00ff')
    await state.coverImage()
    expect(ipc.getCoverImage).toHaveBeenLastCalledWith('S', null)
  })

  it('coverThumbnail asks for the small picture; a failure is null and leaves the error alone', async () => {
    const state = createTagEditorState('S')
    ipc.getCoverThumbnail.mockResolvedValue({ ok: true, value: 'data:image/png;base64,x' })
    expect(await state.coverThumbnail('k')).toBe('data:image/png;base64,x')
    expect(ipc.getCoverThumbnail).toHaveBeenCalledWith('S', 'k')
    ipc.getCoverThumbnail.mockResolvedValue({ ok: false, error: { kind: 'coverTimedOut' } })
    expect(await state.coverThumbnail('k')).toBeNull()
    expect(state.error).toBeNull()
  })

  it('coverChoices lists every cover; a failure is null and leaves the error alone', async () => {
    const choices = [{ key: 'k', mime: 'image/png', bytes: 3, width: null, height: null, files: 2 }]
    ipc.listTagCovers.mockResolvedValue({ ok: true, value: choices })
    const state = createTagEditorState('S')
    expect(await state.coverChoices()).toEqual(choices)
    expect(ipc.listTagCovers).toHaveBeenCalledWith('S')
    ipc.listTagCovers.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    expect(await state.coverChoices()).toBeNull()
    expect(state.error).toBeNull()
  })

  it('useCoverForAll, coverFromBytes and coverFromPath replace the view; a refusal keeps it and is the error', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.useCoverForAll.mockResolvedValue({ ok: true, value: EDITED })
    const state = createTagEditorState('S')
    await state.open()
    await state.useCoverForAll('k')
    expect(ipc.useCoverForAll).toHaveBeenCalledWith('S', 'k')
    expect(state.view).toBe(EDITED)

    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.setCoverFromBytes.mockResolvedValue({ ok: false, error: { kind: 'coverNotAnImage' } })
    await state.coverFromBytes(new Uint8Array([1, 2]))
    expect(ipc.setCoverFromBytes).toHaveBeenCalledWith('S', new Uint8Array([1, 2]))
    expect(state.view).toBe(EDITED)
    expect(state.error).toEqual({ kind: 'coverNotAnImage' })

    ipc.setCoverFromPath.mockResolvedValue({ ok: true, value: VIEW })
    await state.coverFromPath('/d/front.png')
    expect(ipc.setCoverFromPath).toHaveBeenCalledWith('S', '/d/front.png')
    expect(state.view).toBe(VIEW)
    expect(state.error).toBeNull()
  })
})
