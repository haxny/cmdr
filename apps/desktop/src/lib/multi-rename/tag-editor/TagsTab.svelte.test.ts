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
  shared: [
    {
      field: 'album',
      value: 'multiple',
      choices: [
        { values: ['Book one'], files: 2 },
        { values: ['Book two'], files: 1 },
      ],
    },
  ],
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

/** The mock stands in for the state; only what the tab reads is there. */
function asTagState(mock: object): TagEditorState {
  return mock as TagEditorState
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
    undoable: null,
    settled: vi.fn(() => Promise.resolve()),
    tool: vi.fn(() => Promise.resolve()),
  }
  const mounted = await mountInto(TagsTab, { state: asTagState(state) })
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
    [{ kind: 'coverNotAnImage' }, 'That file isn’t a JPEG, PNG, or WebP picture, whatever its name says.'],
    [{ kind: 'unknownTrack' }, 'Couldn’t apply that change.'],
  ] as const)('a failed edit (%j) over the tags says %j', async (error, text) => {
    const { root } = await mountTab({ error })
    expect(root.querySelector('[role="alert"]')?.textContent).toBe(text)
    expect(root.querySelector('table')).not.toBeNull()
  })

  it('a tool waits for an edit in flight, then runs', async () => {
    const { root, state } = await mountTab()
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Strip track prefix')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.settled).toHaveBeenCalled()
    expect(state.tool).toHaveBeenCalledWith({ kind: 'stripTrackPrefix' })
  })

  it('Case changes the field whose box last had focus', async () => {
    const { root, state } = await mountTab()
    byId(root, 'tag-field-album').dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Case')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'lower case')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({ kind: 'case', case: 'lower', fields: ['album'] })
  })

  it('Case forgets a field once focus moves elsewhere in the tab, but not into the tools bar', async () => {
    const { root, state } = await mountTab()
    const buttons = () => [...root.querySelectorAll('button')]
    const named = (text: string) => {
      const found = buttons().find((b) => b.textContent.trim() === text)
      if (!found) throw new Error(`no ${text} button`)
      return found
    }
    const focusIn = (el: Element) => el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    focusIn(byId(root, 'tag-field-album'))
    focusIn(named('Case'))
    named('Case').click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(root.textContent).toContain('Changes: Album')
    // Focus on the Write tags… button: the album box was left, so Case takes the name fields again.
    const write = buttons().find((b) => b.textContent.includes('Write tags…'))
    if (!write) throw new Error('no Write tags… button')
    focusIn(write)
    await new Promise((resolve) => setTimeout(resolve, 0))
    named('lower case').click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({
      kind: 'case',
      case: 'lower',
      fields: ['title', 'album', 'artist', 'albumArtist', 'performer'],
    })
  })

  it.each([[{ kind: 'undo', error: { type: 'notSupported' } }], [{ kind: 'noAppDataDir' }]] as const)(
    'an undo store refusal (%j) is worded as such',
    async (error) => {
      const { root } = await mountTab({ error })
      expect(root.querySelector('[role="alert"]')?.textContent).toBe(
        'Couldn’t use the store that keeps the old tags for Undo.',
      )
    },
  )

  it('read-only says why and locks every box and button', async () => {
    const { root } = await mountTab({ view: { ...VIEW, editable: false, readOnlyReason: 'remoteVolume' } })
    expect(root.textContent).toContain('Tags can only be edited for files on a local disk.')
    for (const box of root.querySelectorAll<HTMLInputElement>('input')) expect(box.disabled).toBe(true)
    for (const button of root.querySelectorAll('button')) expect(button.disabled).toBe(true)
  })
})
