/**
 * The Tags tab's state: each call keeps the view the backend answered with, a typed error stays as it came,
 * `busy` brackets every call, and Undo reverses exactly the last apply that can be reversed.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'

const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  editTags: vi.fn(),
  runTagTool: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  undoTagApply: vi.fn(),
  closeTagEditor: vi.fn(),
  listCoverCandidates: vi.fn(),
  setCoverFromFile: vi.fn(),
  getCoverImage: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
const warn = vi.hoisted(() => vi.fn())
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

import { createTagEditorState } from './tag-editor-state.svelte'
import { getLastTagApply, setLastTagApply } from './last-tag-apply.svelte'

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
  setLastTagApply(null)
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

  it('open twice asks the backend once', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    await Promise.all([state.open(), state.open()])
    await state.open()
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(1)
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
    expect(ipc.runTagTool).toHaveBeenCalledWith('S', { kind: 'stripDiacritics' })
    expect(state.view).toBe(EDITED)
    ipc.runTagTool.mockResolvedValueOnce({ ok: false, error: { kind: 'readOnly' } })
    await state.tool({ kind: 'stripDiacritics' } as never)
    expect(state.error).toEqual({ kind: 'readOnly' })
  })

  it('changes returns the backend answer and sets the error on failure', async () => {
    const state = createTagEditorState('S')
    ipc.getTagChanges.mockResolvedValueOnce({ ok: true, value: [] })
    expect(await state.changes()).toEqual({ ok: true, value: [] })
    ipc.getTagChanges.mockResolvedValueOnce({ ok: false, error: { kind: 'notOpen' } })
    expect((await state.changes()).ok).toBe(false)
    expect(state.error).toEqual({ kind: 'notOpen' })
  })

  it('apply returns the report and reloads the view', async () => {
    ipc.applyTags.mockResolvedValue({ ok: true, value: report() })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    const state = createTagEditorState('S')
    const res = await state.apply(REQUEST)
    expect(ipc.applyTags).toHaveBeenCalledWith('S', REQUEST)
    expect(res).toEqual({ ok: true, value: report() })
    expect(ipc.openTagEditor).toHaveBeenCalledWith('S')
    expect(state.view).toBe(VIEW)
  })

  it('apply keeps the typed error', async () => {
    ipc.applyTags.mockResolvedValue({ ok: false, error: { kind: 'noAppDataDir' } })
    const state = createTagEditorState('S')
    const res = await state.apply(REQUEST)
    expect(res.ok).toBe(false)
    expect(state.error).toEqual({ kind: 'noAppDataDir' })
  })

  it('undo reverses the last apply and then has nothing left', async () => {
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A9' }) })
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.undoTagApply.mockResolvedValue({ ok: true, value: { restored: ['/a.mp3'], missing: [], failed: [] } })
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
      .mockResolvedValueOnce({ ok: true, value: { restored: [], missing: [], failed: [] } })
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
    ipc.undoTagApply.mockResolvedValue({ ok: true, value: { restored: ['/x.mp3'], missing: [], failed: [] } })
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
    expect(res).toEqual({ ok: true, value: { restored: ['/x.mp3', '/x.mp3'], missing: [], failed: [] } })
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
    setLastTagApply({
      applies: [
        { applyId: 'A1', written: 3 },
        { applyId: 'A2', written: 1 },
      ],
      partial: false,
    })
    ipc.undoTagApply
      .mockResolvedValueOnce({ ok: true, value: { restored: ['/b.mp3'], missing: [], failed: [] } })
      .mockResolvedValueOnce({ ok: false, error: { kind: 'taskFailed' } })
    const state = createTagEditorState('S')
    await state.open()
    const res = await state.undo()
    // What it restored before it stopped comes with the error, and the retry counts only what's left.
    expect(res).toEqual({
      ok: false,
      error: { kind: 'taskFailed' },
      restored: { restored: ['/b.mp3'], missing: [], failed: [] },
    })
    expect(state.error).toEqual({ kind: 'taskFailed' })
    expect(getLastTagApply()).toEqual({ applies: [{ applyId: 'A1', written: 3 }], partial: false })
    expect(ipc.openTagEditor).toHaveBeenCalledTimes(2)
  })

  it('an apply whose files could not all be restored stays undoable', async () => {
    setLastTagApply({ applies: [{ applyId: 'A1', written: 2 }], partial: false })
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: ['/a.mp3'], missing: [], failed: [['/b.mp3', { type: 'notSupported' }]] },
    })
    const state = createTagEditorState('S')
    expect((await state.undo()).ok).toBe(true)
    // Only the file it couldn't restore is left for a retry.
    expect(getLastTagApply()).toEqual({ applies: [{ applyId: 'A1', written: 1 }], partial: false })
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

  it('the write outlives the sheet: a new state of a later sheet can undo it', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
    ipc.applyTags.mockResolvedValue({ ok: true, value: report({ applyId: 'A7' }) })
    ipc.undoTagApply.mockResolvedValue({ ok: true, value: { restored: [], missing: [], failed: [] } })
    const first = createTagEditorState('S')
    await first.apply(REQUEST)
    await first.close()
    const later = createTagEditorState('T')
    expect(later.undoable).toEqual({ applies: [{ applyId: 'A7', written: 1 }], partial: false })
    await later.undo()
    expect(ipc.undoTagApply).toHaveBeenCalledWith('A7', 'T')
    // The later sheet never showed its Tags tab, so the undo opens no tag session for it.
    expect(ipc.openTagEditor).not.toHaveBeenCalledWith('T')
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

  it('coverImage returns the bytes, or null (logged by kind) without touching busy or the error', async () => {
    const state = createTagEditorState('S')
    ipc.getCoverImage.mockResolvedValue({ ok: true, value: { mime: 'image/png', data: [1] } })
    expect(await state.coverImage()).toEqual({ mime: 'image/png', data: [1] })
    ipc.getCoverImage.mockResolvedValue({ ok: false, error: { kind: 'noSession' } })
    expect(await state.coverImage()).toBeNull()
    expect(state.error).toBeNull()
    expect(warn).toHaveBeenCalled()
  })
})
