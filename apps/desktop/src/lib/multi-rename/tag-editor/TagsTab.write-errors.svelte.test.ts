/**
 * A list or a write the backend refuses is worded once, by the Apply panel next to its buttons, never a second time
 * at the top of the tab ("Couldn't apply that change." was wrong for a write and stayed until a later success).
 * The real state over mocked IPC, so the shared `error` is the one the tab reads.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { TagEditorView } from '#lib/tauri-commands/index.js'

const ipc = vi.hoisted(() => ({
  openTagEditor: vi.fn(),
  getTagChanges: vi.fn(),
  applyTags: vi.fn(),
  getCoverImage: vi.fn(),
  closeTagEditor: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', async (original) => ({ ...(await original<object>()), ...ipc }))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

import TagsTab from './TagsTab.svelte'
import { createTagEditorState } from './tag-editor-state.svelte'
import { setLastTagApply } from './last-tag-apply.svelte'
import { mountInto } from './tag-cell.test-helper'
import { press, settle } from './apply-panel.test-helper'

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [],
  tracks: [
    {
      path: '/b/01.mp3',
      fileName: '01.mp3',
      track: '1',
      title: 'Prvni',
      durationMs: 1000,
      changed: true,
      readError: null,
    },
  ],
  cover: { kind: 'empty' },
  extras: [],
  dirty: true,
  diacritics: [],
  badDates: 0,
}

const EDIT_FAILED = 'Couldn’t apply that change.'

let cleanup: (() => void) | undefined
beforeEach(() => {
  vi.resetAllMocks()
  setLastTagApply(null)
  ipc.openTagEditor.mockResolvedValue({ ok: true, value: VIEW })
  ipc.getCoverImage.mockResolvedValue({ ok: true, value: null })
  ipc.closeTagEditor.mockResolvedValue(undefined)
  ipc.getTagChanges.mockResolvedValue({
    ok: true,
    value: [{ path: '/b/01.mp3', fileName: '01.mp3', fields: ['album'], cover: false, extras: false, removes: [] }],
  })
})
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

async function mountTab(): Promise<HTMLElement> {
  const mounted = await mountInto(TagsTab, { state: createTagEditorState('S') })
  cleanup = mounted.cleanup
  await settle()
  return mounted.root
}

const timesShown = (root: HTMLElement, text: string): number => root.textContent.split(text).length - 1

describe('TagsTab words a refused list or write once', () => {
  it.each([
    [{ kind: 'notOpen' }, 'The tags aren’t open any more. Close Multi-Rename and open it again.'],
    [{ kind: 'noSession' }, 'The tags aren’t open any more. Close Multi-Rename and open it again.'],
    [{ kind: 'readOnly' }, 'Tags can only be edited for files on a local disk.'],
    [{ kind: 'noAppDataDir' }, 'Couldn’t use the store that keeps the old tags for Undo.'],
    [{ kind: 'taskFailed' }, 'The write stopped before it finished, so some files may not have their new tags.'],
  ] as const)('a write refused with %j shows its reason once', async (error, reason) => {
    ipc.applyTags.mockResolvedValue({ ok: false, error })
    const root = await mountTab()

    await press(root, 'Write tags…')
    await press(root, 'Write tags')

    expect(timesShown(root, reason)).toBe(1)
    expect(root.textContent).not.toContain(EDIT_FAILED)
  })

  it('a change list that could not be made shows its reason once', async () => {
    ipc.getTagChanges.mockResolvedValue({ ok: false, error: { kind: 'taskFailed' } })
    const root = await mountTab()

    await press(root, 'Write tags…')

    expect(timesShown(root, 'Cmdr couldn’t check what will change. Try again in a moment.')).toBe(1)
    expect(root.textContent).not.toContain(EDIT_FAILED)
  })
})
