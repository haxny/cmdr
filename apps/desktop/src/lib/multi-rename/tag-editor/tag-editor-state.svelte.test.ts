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
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)

import { createTagEditorState } from './tag-editor-state.svelte'

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
    expect(ipc.undoTagApply).toHaveBeenCalledWith('A9')
    expect(res.ok).toBe(true)
    expect(await state.undo()).toEqual({ ok: false, error: { kind: 'nothingToUndo' } })
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
    expect(ipc.undoTagApply).toHaveBeenNthCalledWith(2, 'A5')
  })

  it('undo with no apply is a typed refusal and no IPC', async () => {
    const state = createTagEditorState('S')
    expect(await state.undo()).toEqual({ ok: false, error: { kind: 'nothingToUndo' } })
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
    expect(await state.undo()).toEqual({ ok: false, error: { kind: 'undoUnavailable' } })
    expect(ipc.undoTagApply).not.toHaveBeenCalled()
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

  it('close before open does not touch the backend', async () => {
    const state = createTagEditorState('S')
    await state.close()
    expect(ipc.closeTagEditor).not.toHaveBeenCalled()
  })
})
