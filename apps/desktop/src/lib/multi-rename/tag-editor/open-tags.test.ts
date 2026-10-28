/**
 * How the main window opens a Tags window: from the focused pane's selection (`file.editTags`) or from the
 * Multi-Rename sheet's files ("Tags…"). Each open makes a NEW Multi-Rename session the window holds, so the sheet
 * closing never ends it. Off (the setting) or with nothing selected, nothing opens and no IPC goes out.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const flags = vi.hoisted(() => ({ tagEditor: true }))
const ipc = vi.hoisted(() => ({
  openTagsSession: vi.fn(),
  reopenForTags: vi.fn(),
  holdTagsSession: vi.fn(),
  releaseTagsSession: vi.fn(() => Promise.resolve()),
  closeMultiRename: vi.fn(() => Promise.resolve()),
}))
const openTagsWindow = vi.hoisted(() =>
  vi.fn((_sessionId: string, _folder: string, _onFailed: () => void) => Promise.resolve()),
)
const addToast = vi.hoisted(() => vi.fn())

vi.mock('#lib/tauri-commands/index.js', () => ipc)
vi.mock('#lib/settings/index.js', () => ({
  getSetting: (id: string) => (id === 'multiRename.tagEditor' ? flags.tagEditor : false),
}))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getShowHiddenFiles: () => false }))
vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))
vi.mock('./tags-window', () => ({ openTagsWindow }))

import { openTagsForSelection, openTagsForSession, tagsOpenRefusal, type TagsSelection } from './open-tags'

const SELECTION: TagsSelection = {
  listingId: 'L',
  selectedIndices: [2, 3],
  expectedSequence: 9,
  nothingSelected: false,
}

function explorer(target: TagsSelection | null = SELECTION) {
  return { getFocusedPaneRenameTarget: vi.fn(() => target) }
}

describe('opening a Tags window', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    flags.tagEditor = true
    ipc.openTagsSession.mockResolvedValue({ ok: true, value: { sessionId: 'multi-rename-5', count: 2 } })
    ipc.reopenForTags.mockResolvedValue({ ok: true, value: { sessionId: 'multi-rename-6', count: 2 } })
    ipc.holdTagsSession.mockResolvedValue({ ok: true, value: { tracks: 2, folderName: 'Dracula' } })
  })

  it('file.editTags refuses with the setting off or with nothing selected', () => {
    expect(tagsOpenRefusal(false, SELECTION)).toBe('off')
    expect(tagsOpenRefusal(true, { ...SELECTION, selectedIndices: null, nothingSelected: true })).toBe(
      'nothingSelected',
    )
    expect(tagsOpenRefusal(true, null)).toBe('nothingSelected')
    expect(tagsOpenRefusal(true, SELECTION)).toBeNull()
    // "Select all" is a selection (the backend gets `null` for every row).
    expect(tagsOpenRefusal(true, { ...SELECTION, selectedIndices: null })).toBeNull()
  })

  it('with the setting off nothing opens and no IPC goes out', async () => {
    flags.tagEditor = false
    await openTagsForSelection(explorer())
    await openTagsForSession('multi-rename-1')
    for (const call of Object.values(ipc)) expect(call).not.toHaveBeenCalled()
    expect(openTagsWindow).not.toHaveBeenCalled()
  })

  it('with nothing selected nothing opens, and a toast says why', async () => {
    await openTagsForSelection(explorer({ ...SELECTION, selectedIndices: null, nothingSelected: true }))
    expect(ipc.openTagsSession).not.toHaveBeenCalled()
    expect(openTagsWindow).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith('Select the audio files to tag first.', { level: 'info' })
  })

  it('opens a fresh session from the selection, holds it, then opens its window', async () => {
    await openTagsForSelection(explorer())

    expect(ipc.openTagsSession).toHaveBeenCalledWith('L', false, [2, 3], 9)
    expect(ipc.holdTagsSession).toHaveBeenCalledWith('multi-rename-5')
    expect(openTagsWindow).toHaveBeenCalledWith('multi-rename-5', 'Dracula', expect.any(Function))
    expect(ipc.closeMultiRename).not.toHaveBeenCalled()
  })

  it('a selection without a single audio file it can tag says so and ends the session', async () => {
    ipc.holdTagsSession.mockResolvedValue({ ok: true, value: { tracks: 0, folderName: 'Photos' } })

    await openTagsForSelection(explorer())

    expect(openTagsWindow).not.toHaveBeenCalled()
    expect(ipc.releaseTagsSession).toHaveBeenCalledWith('multi-rename-5')
    expect(ipc.closeMultiRename).toHaveBeenCalledWith('multi-rename-5')
    expect(addToast).toHaveBeenCalledWith('None of the selected files is an audio file Cmdr can tag.', {
      level: 'info',
    })
  })

  it('a selection the listing moved past is refused as Multi-Rename refuses it', async () => {
    ipc.openTagsSession.mockResolvedValue({ ok: false, error: { type: 'selectionChanged', listingId: 'L' } })

    await openTagsForSelection(explorer())

    expect(openTagsWindow).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('changed'), { level: 'warn' })
  })

  it('a window that fails to open gives its session back', async () => {
    openTagsWindow.mockRejectedValueOnce(new Error('no window'))

    await openTagsForSelection(explorer())

    expect(ipc.releaseTagsSession).toHaveBeenCalledWith('multi-rename-5')
    expect(ipc.closeMultiRename).toHaveBeenCalledWith('multi-rename-5')
  })

  it('a creation Tauri refuses later (`tauri://error`, after the open resolved) gives its session back', async () => {
    openTagsWindow.mockImplementationOnce((_id, _folder, onFailed) => {
      setTimeout(onFailed, 0)
      return Promise.resolve()
    })

    await openTagsForSelection(explorer())
    expect(ipc.releaseTagsSession).not.toHaveBeenCalled()
    await new Promise((resolve) => setTimeout(resolve, 5))

    expect(ipc.releaseTagsSession).toHaveBeenCalledWith('multi-rename-5')
    expect(ipc.closeMultiRename).toHaveBeenCalledWith('multi-rename-5')
  })

  it('from the sheet it copies the sheet’s session, never handing the window the sheet’s own', async () => {
    await openTagsForSession('multi-rename-1')

    expect(ipc.reopenForTags).toHaveBeenCalledWith('multi-rename-1')
    expect(ipc.holdTagsSession).toHaveBeenCalledWith('multi-rename-6')
    expect(openTagsWindow).toHaveBeenCalledWith('multi-rename-6', 'Dracula', expect.any(Function))
    expect(ipc.openTagsSession).not.toHaveBeenCalled()
  })

  it('a double press (the menu accelerator and the keydown both fire) opens one window', async () => {
    await Promise.all([openTagsForSelection(explorer()), openTagsForSelection(explorer())])
    expect(ipc.openTagsSession).toHaveBeenCalledTimes(1)
    expect(openTagsWindow).toHaveBeenCalledTimes(1)
  })
})
