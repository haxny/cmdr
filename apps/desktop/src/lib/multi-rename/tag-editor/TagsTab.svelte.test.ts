/** The Tags tab wired to its state: edits reach `state.edit`, a failed edit is worded by its typed kind, read-only locks it. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import TagsTab from './TagsTab.svelte'
import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'
import { byId, leave, mountInto, typeInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({ getFileSizeFormat: () => 'binary' }))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [{ field: 'album', value: 'multiple' }],
  tracks: [
    {
      path: '/b/01.mp3',
      fileName: '01.mp3',
      track: '1',
      title: 'Prvni',
      durationMs: 1000,
      changed: false,
      readError: null,
    },
    {
      path: '/b/02.mp3',
      fileName: '02.mp3',
      track: '2',
      title: 'Druha',
      durationMs: 1000,
      changed: false,
      readError: null,
    },
  ],
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
}

async function mountTab(over: { view?: TagEditorView; error?: TagStateError | null } = {}) {
  const state = {
    view: over.view ?? VIEW,
    error: over.error ?? null,
    busy: false,
    open: vi.fn(() => Promise.resolve()),
    edit: vi.fn(() => Promise.resolve()),
    coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [{ fileName: 'front.jpg', size: 10 }] })),
    coverFromFile: vi.fn(() => Promise.resolve()),
    coverImage: vi.fn(() => Promise.resolve(null)),
  }
  const mounted = await mountInto(TagsTab, { state: state as unknown as TagEditorState })
  cleanup = mounted.cleanup
  return { root: mounted.root, state }
}

describe('TagsTab', () => {
  it('sends a shared edit and a track edit to the state', async () => {
    const { root, state } = await mountTab()
    await typeInto(byId<HTMLInputElement>(root, 'tag-field-album'), 'Dracula')
    await leave(byId(root, 'tag-field-album'))
    await typeInto(byId<HTMLInputElement>(root, 'tag-track-title-1'), 'Konec')
    await leave(byId(root, 'tag-track-title-1'))
    expect(state.edit.mock.calls).toEqual([
      [[{ kind: 'setAll', field: 'album', values: ['Dracula'] }]],
      [[{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: ['Konec'] }]],
    ])
  })

  it('the cover picker lists the folder and a pick goes to the state', async () => {
    const { root, state } = await mountTab()
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.includes('From folder'))?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    root.querySelector<HTMLButtonElement>('.candidate')?.click()
    expect(state.coverFromFile).toHaveBeenCalledWith('front.jpg')
  })

  it.each([
    [{ kind: 'coverTooLarge' }, 'That picture is too large (over 10 MB).'],
    [{ kind: 'coverUnreadable' }, 'Couldn’t read that picture, or it isn’t a supported image.'],
    [{ kind: 'badCoverName' }, 'Couldn’t read that picture, or it isn’t a supported image.'],
    [{ kind: 'unknownTrack' }, 'Couldn’t apply that change.'],
  ] as const)('a failed edit (%j) over the tags says %j', async (error, text) => {
    const { root } = await mountTab({ error })
    expect(root.querySelector('[role="alert"]')?.textContent).toBe(text)
    expect(root.querySelector('table')).not.toBeNull()
  })

  it('read-only says why and locks every box and button', async () => {
    const { root } = await mountTab({ view: { ...VIEW, editable: false, readOnlyReason: 'remoteVolume' } })
    expect(root.textContent).toContain('Tags can only be edited for files on a local disk.')
    for (const box of root.querySelectorAll<HTMLInputElement>('input')) expect(box.disabled).toBe(true)
    for (const button of root.querySelectorAll('button')) expect(button.disabled).toBe(true)
  })
})
