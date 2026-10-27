/** Write, then Undo tag changes, through the real state: the IPC undoes exactly the write's apply ids. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import ApplyPanel from './ApplyPanel.svelte'
import { createTagEditorState } from './tag-editor-state.svelte'
import { setLastTagApply } from './last-tag-apply.svelte'
import { mountInto } from './tag-cell.test-helper'
import { CHANGES, PANEL_VIEW, applyReport, buttonNamed, press } from './apply-panel.test-helper'

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
const confirm = vi.hoisted(() => vi.fn((_message: string, _title?: string, _label?: string) => Promise.resolve(true)))
vi.mock('#lib/utils/confirm-dialog.js', () => ({ confirmDialog: confirm }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
  setLastTagApply(null)
  vi.resetAllMocks()
})

describe('ApplyPanel undo', () => {
  it('undo_after_write_calls_undo_tag_apply, after a warning that later changes are overwritten', async () => {
    ipc.openTagEditor.mockResolvedValue({ ok: true, value: PANEL_VIEW })
    ipc.getTagChanges.mockResolvedValue({ ok: true, value: CHANGES })
    ipc.applyTags
      .mockResolvedValueOnce({
        ok: true,
        value: applyReport({
          applyId: 'A1',
          written: ['/b/01.mp3'],
          wouldDrop: [{ path: '/b/02.mp3', items: ['TSOP'] }],
        }),
      })
      .mockResolvedValueOnce({ ok: true, value: applyReport({ applyId: 'A2', written: ['/b/02.mp3'] }) })
    ipc.undoTagApply.mockResolvedValue({ ok: true, value: { restored: ['/b/01.mp3'], missing: [], failed: [] } })
    confirm.mockResolvedValue(true)
    const tags = createTagEditorState('S')
    await tags.open()
    const mounted = await mountInto(ApplyPanel, { tags })
    cleanup = mounted.cleanup
    const { root } = mounted

    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    await press(root, 'Write anyway')
    expect(() => buttonNamed(root, 'Undo tag changes')).not.toThrow()

    await press(root, 'Undo tag changes')
    expect(confirm).toHaveBeenCalledWith(
      'This puts the tags of the 2 files the last write changed back as they were before it. Any change made to those files since the write is overwritten.',
      'Undo tag changes?',
      'Undo',
    )
    expect(ipc.undoTagApply.mock.calls).toEqual([['A2'], ['A1']])
    expect(root.textContent).toContain('Tags restored in 2 files')
    // Undone: nothing left to undo until the next write.
    expect(() => buttonNamed(root, 'Undo tag changes')).toThrow()
  })
})
