/** The Tags tab wired to its state: edits reach `state.edit`, a failed edit is worded by its typed kind, read-only locks it. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
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
  badDates: 0,
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
    textTool: vi.fn(() => Promise.resolve()),
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
    expect(state.tool).toHaveBeenCalledWith({ kind: 'stripTrackPrefix' }, null)
  })

  it('with 2 of 4 tracks checked, Renumber works on those two only', async () => {
    const tracks = ['01', '02', '03', '04'].map((n) => ({
      ...VIEW.tracks[0],
      path: `/b/${n}.mp3`,
      fileName: `${n}.mp3`,
    })) as TagEditorView['tracks']
    const { root, state } = await mountTab({ view: { ...VIEW, tracks } })
    root.querySelector<HTMLInputElement>('input[aria-label="Select 02.mp3"]')?.click()
    root.querySelector<HTMLInputElement>('input[aria-label="Select 04.mp3"]')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Renumber')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, ['/b/02.mp3', '/b/04.mp3'])
  })

  it('a view reload drops the checked tracks that are gone for good', async () => {
    const state = $state({
      view: VIEW,
      error: null,
      busy: false,
      open: vi.fn(() => Promise.resolve()),
      edit: vi.fn(() => Promise.resolve()),
      coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
      coverImage: vi.fn(() => Promise.resolve(null)),
      undoable: null,
      settled: vi.fn(() => Promise.resolve()),
      tool: vi.fn(() => Promise.resolve()),
    })
    const mounted = await mountInto(TagsTab, { state: asTagState(state) })
    cleanup = mounted.cleanup
    const second = () => mounted.root.querySelector<HTMLInputElement>('input[aria-label="Select 02.mp3"]')
    second()?.click()
    await tick()
    expect(second()?.checked).toBe(true)

    state.view = { ...VIEW, tracks: VIEW.tracks.slice(0, 1) }
    await tick()
    expect(second()).toBeNull()

    state.view = VIEW
    await tick()
    expect(second()?.checked).toBe(false)
  })

  it('when every checked track vanished, the tool does not run and a notice says so; the next run takes all', async () => {
    const state = $state({
      view: VIEW,
      error: null,
      busy: false,
      open: vi.fn(() => Promise.resolve()),
      edit: vi.fn(() => Promise.resolve()),
      coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
      coverImage: vi.fn(() => Promise.resolve(null)),
      undoable: null,
      settled: vi.fn(() => Promise.resolve()),
      tool: vi.fn(() => Promise.resolve()),
    })
    const mounted = await mountInto(TagsTab, { state: asTagState(state) })
    cleanup = mounted.cleanup
    const renumber = () =>
      [...mounted.root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Renumber')?.click()
    const settle = () => new Promise((resolve) => setTimeout(resolve, 0))
    mounted.root.querySelector<HTMLInputElement>('input[aria-label="Select 02.mp3"]')?.click()
    await tick()
    const renamed = VIEW.tracks.map((t) => ({ ...t, path: `${t.path}.new` }))
    state.view = { ...VIEW, tracks: renamed }
    await tick()

    renumber()
    await settle()
    expect(state.tool).not.toHaveBeenCalled()
    expect(mounted.root.textContent).toContain('The checked tracks are no longer here, so nothing was changed.')

    renumber()
    await settle()
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, null)
    expect(mounted.root.textContent).not.toContain('no longer here')
  })

  it('the Find and replace panel works on the checked tracks alone', async () => {
    const { root, state } = await mountTab()
    root.querySelector<HTMLInputElement>('input[aria-label="Select 02.mp3"]')?.click()
    await tick()
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Find and replace')?.click()
    await tick()
    await typeInto(byId<HTMLInputElement>(root, 'tag-replace-search'), 'a')
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Replace')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.settled).toHaveBeenCalled()
    expect(state.textTool).toHaveBeenCalledWith(expect.objectContaining({ kind: 'replace' }), ['/b/02.mp3'])
  })

  it.each([
    { kind: 'badRegex' },
    { kind: 'badMask', error: { type: 'unclosed', at: 0 } },
    { kind: 'badPattern', error: { kind: 'noFields' } },
  ] as const)('a %j error is left to the panel that worded it, not the generic notice', async (error) => {
    const { root } = await mountTab({ error })
    expect(root.querySelector('[role="alert"]')).toBeNull()
  })

  it('Case changes the field whose box last had focus', async () => {
    const { root, state } = await mountTab()
    byId(root, 'tag-field-album').dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Case')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    ;[...root.querySelectorAll('button')].find((b) => b.textContent.trim() === 'lower case')?.click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({ kind: 'case', case: 'lower', fields: ['album'] }, null)
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
    expect(state.tool).toHaveBeenCalledWith(
      { kind: 'case', case: 'lower', fields: ['title', 'album', 'artist', 'albumArtist', 'performer'] },
      null,
    )
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
    // Once, at the top: it explains every locked box and button, Write tags… included.
    expect(root.textContent.split('Tags can only be edited for files on a local disk.')).toHaveLength(2)
    for (const box of root.querySelectorAll<HTMLInputElement>('input')) expect(box.disabled).toBe(true)
    // Playing writes nothing, so the play buttons and Open in foobar2000 stay available.
    const writers = [...root.querySelectorAll('button')].filter(
      (b) => !b.classList.contains('play') && b.textContent.trim() !== 'Open in foobar2000',
    )
    for (const button of writers) expect(button.disabled).toBe(true)
  })
})
