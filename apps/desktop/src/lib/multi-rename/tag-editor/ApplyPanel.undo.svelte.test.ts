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
  it('undo_after_write_calls_undo_tag_apply, after saying a file changed since is left alone', async () => {
    // Dirty until the write; the view each apply reads back holds what's on disk.
    ipc.openTagEditor
      .mockResolvedValueOnce({ ok: true, value: PANEL_VIEW })
      .mockResolvedValue({ ok: true, value: { ...PANEL_VIEW, dirty: false } })
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
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: ['/b/01.mp3'], missing: [], changed: [], failed: [] },
    })
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
      'This puts the tags of the 2 files the last write changed back as they were before it. A file that changed since the write is left as it is.',
      'Undo tag changes?',
      'Undo',
    )
    expect(ipc.undoTagApply.mock.calls).toEqual([
      ['A2', 'S'],
      ['A1', 'S'],
    ])
    expect(root.textContent).toContain('Tags restored in 2 files')
    // Undone: nothing left to undo until the next write.
    expect(() => buttonNamed(root, 'Undo tag changes')).toThrow()
  })

  it('says which files changed since the write (a rename swapped them) and were left alone, not restored', async () => {
    ipc.openTagEditor
      .mockResolvedValueOnce({ ok: true, value: PANEL_VIEW })
      .mockResolvedValue({ ok: true, value: { ...PANEL_VIEW, dirty: false } })
    ipc.getTagChanges.mockResolvedValue({ ok: true, value: CHANGES })
    ipc.applyTags.mockResolvedValue({
      ok: true,
      value: applyReport({ applyId: 'A1', written: ['/b/01.mp3', '/b/02.mp3'] }),
    })
    ipc.undoTagApply.mockResolvedValue({
      ok: true,
      value: { restored: [], missing: [], changed: ['/b/01.mp3', '/b/02.mp3'], failed: [] },
    })
    confirm.mockResolvedValue(true)
    const tags = createTagEditorState('S')
    await tags.open()
    const mounted = await mountInto(ApplyPanel, { tags })
    cleanup = mounted.cleanup
    const { root } = mounted

    await press(root, 'Write tags…')
    await press(root, 'Write tags')
    await press(root, 'Undo tag changes')

    expect(root.textContent).toContain(
      '2 files changed after the write, for example by a rename, so they were left as they are.',
    )
    expect(root.textContent).not.toContain('Tags restored')
    expect(root.textContent).not.toContain('couldn’t be restored')
  })
})
