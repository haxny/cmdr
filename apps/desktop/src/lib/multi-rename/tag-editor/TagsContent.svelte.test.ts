/**
 * The Tags window's content wired to its state: the two panes' edits reach `state.edit`, the Tracks pane's selection
 * scopes the tools, a failed edit is worded by its typed kind, read-only locks it.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'
import type { CoverChoice } from '#lib/ipc/bindings.js'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import TagsContent from './TagsContent.svelte'
import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'
import { byId, editCursorRow, mountInto, paneList, press, toggleTrack, typeInto } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  getRowHeight: () => 20,
  getStripedRows: () => true,
}))

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
      choicesMore: 0,
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
  sort: null,
}

/** The mock stands in for the state; only what the window reads is there. */
function asTagState(mock: object): TagEditorState {
  return mock as TagEditorState
}

async function mountTab(over: { view?: TagEditorView; error?: TagStateError | null; choices?: CoverChoice[] } = {}) {
  const state = {
    view: over.view ?? VIEW,
    error: over.error ?? null,
    busy: false,
    open: vi.fn(() => Promise.resolve()),
    edit: vi.fn(() => Promise.resolve()),
    coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [{ fileName: 'front.jpg', size: 10 }] })),
    coverFromFile: vi.fn(() => Promise.resolve()),
    coverImage: vi.fn((key?: string | null) =>
      Promise.resolve(key === 'k1' ? 'data:image/png;base64,iVBORw0KGgo=' : null),
    ),
    coverChoices: vi.fn(() => Promise.resolve(over.choices ?? [])),
    coverThumbnail: vi.fn(() => Promise.resolve('data:image/png;base64,small')),
    useCoverForAll: vi.fn(() => Promise.resolve()),
    coverFromBytes: vi.fn(() => Promise.resolve()),
    coverFromPath: vi.fn(() => Promise.resolve()),
    undoable: null,
    settled: vi.fn(() => Promise.resolve()),
    tool: vi.fn(() => Promise.resolve()),
    textTool: vi.fn(() => Promise.resolve()),
    sortTracks: vi.fn(() => Promise.resolve()),
  }
  const mounted = await mountInto(TagsContent, { state: asTagState(state) })
  cleanup = mounted.cleanup
  return { root: mounted.root, state }
}

/** Presses a function-key bar button by its label (Shift held for the Shift row), as a click would. */
async function bar(root: HTMLElement, label: string, shift = false): Promise<void> {
  if (shift) document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
  await tick()
  const button = [...root.querySelectorAll<HTMLButtonElement>('[role="toolbar"] button')].find(
    (b) => b.querySelector('span')?.textContent === label,
  )
  if (!button) throw new Error(`no ${label} key`)
  button.click()
  if (shift) document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift' }))
  await new Promise((resolve) => setTimeout(resolve, 0))
}

/** A button in the open dialog, by its text. */
function inDialog(text: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(
    (b) => b.textContent.trim() === text,
  )
  if (!found) throw new Error(`no ${text} in the dialog`)
  return found
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('TagsContent', () => {
  it('sends a shared edit and a track edit to the state', async () => {
    const { root, state } = await mountTab()
    const album = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(album, 'Dracula')
    await press(album, 'Enter')
    const tracks = paneList(root, 'Tracks')
    await press(tracks, 'ArrowDown')
    const title = await editCursorRow(tracks)
    await typeInto(title, 'Konec')
    await press(title, 'Enter')
    expect(state.edit.mock.calls).toEqual([
      [[{ kind: 'setAll', field: 'album', values: ['Dracula'] }]],
      [[{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: ['Konec'] }]],
    ])
  })

  it('a Tracks column header sorts through the state, flipping the direction of the view’s active sort', async () => {
    const { root, state } = await mountTab({ view: { ...VIEW, sort: { key: 'title', descending: false } } })
    const header = (label: string) =>
      [...root.querySelectorAll<HTMLButtonElement>('.sortable-header')].find((b) => b.textContent.includes(label))
    header('Title')?.click()
    expect(state.sortTracks).toHaveBeenLastCalledWith({ key: 'title', descending: true })
    header('Length')?.click()
    expect(state.sortTracks).toHaveBeenLastCalledWith({ key: 'length', descending: false })
  })

  it('the Tracks cursor follows its track when the order changes', async () => {
    const state = $state({
      view: VIEW,
      error: null,
      busy: false,
      open: vi.fn(() => Promise.resolve()),
      edit: vi.fn(() => Promise.resolve()),
      coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
      coverImage: vi.fn(() => Promise.resolve(null)),
      coverChoices: vi.fn(() => Promise.resolve([])),
      coverThumbnail: vi.fn(() => Promise.resolve(null)),
      undoable: null,
      settled: vi.fn(() => Promise.resolve()),
      tool: vi.fn(() => Promise.resolve()),
    })
    const mounted = await mountInto(TagsContent, { state: asTagState(state) })
    cleanup = mounted.cleanup
    const cursorPath = () =>
      mounted.root.querySelector<HTMLElement>('[role="option"].is-under-cursor[data-track-path]')?.dataset.trackPath
    mounted.root
      .querySelector('[data-track-path="/b/02.mp3"]')
      ?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    await tick()
    expect(cursorPath()).toBe('/b/02.mp3')

    state.view = { ...VIEW, tracks: [...VIEW.tracks].reverse() }
    await tick()
    expect(cursorPath()).toBe('/b/02.mp3')
  })

  it("passes the view's bad-date count on, so the Date row is flagged on open", async () => {
    const view: TagEditorView = {
      ...VIEW,
      shared: [{ field: 'date', value: { same: ['2010-20-09'] }, choices: [], choicesMore: 0 }],
      badDates: 3,
    }
    const { root } = await mountTab({ view })
    expect(byId(root, 'tag-album-row-date').classList.contains('is-invalid')).toBe(true)
    expect(root.querySelector('#tag-album-date-hint')).not.toBeNull()
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
    [
      { kind: 'pastedNotAnImage' },
      'That isn’t a picture Cmdr can use as a cover. Copy a JPEG, PNG, WebP, or TIFF picture, then paste again.',
    ],
    [
      { kind: 'coverTimedOut' },
      'The drive that picture is on stopped answering, so the cover is unchanged. Copy the picture to this Mac first, then drop it again.',
    ],
    [
      { kind: 'coverBusy' },
      'Earlier pictures from a drive that stopped answering are still being read. Wait a moment, or copy the picture to this Mac first, then drop it again.',
    ],
    [
      { kind: 'pasteTimedOut' },
      'Converting that picture took too long, so the cover is unchanged. Paste a smaller picture, or a JPEG or PNG.',
    ],
    [{ kind: 'coverNotAFile' }, 'Only a picture file can become the cover, not a folder or a link.'],
    [{ kind: 'unknownCover' }, 'That cover isn’t in the files any more. Pick another one.'],
    [{ kind: 'unknownTrack' }, 'Couldn’t apply that change.'],
  ] as const)('a failed edit (%j) over the tags says %j', async (error, text) => {
    const { root } = await mountTab({ error })
    expect(root.querySelector('[role="alert"]')?.textContent).toBe(text)
    expect(root.querySelector('[role="listbox"]')).not.toBeNull()
  })

  describe('the cover section', () => {
    const FRONT: CoverChoice = { key: 'k1', mime: 'image/png', bytes: 40, width: null, height: null, files: 1 }
    const NONE: CoverChoice = { key: 'none', mime: '', bytes: 0, width: null, height: null, files: 1 }

    it('sits in the Album pane’s scroll, after the rows', async () => {
      const { root } = await mountTab({ choices: [FRONT, NONE] })
      const scroller = root.querySelector('[data-pane="album"] .full-list')
      expect(scroller?.querySelector('.cover-group section.cover')).not.toBeNull()
    })

    it('a click on a cover opens it large; "Use for all files" sends its key and closes the view', async () => {
      const { root, state } = await mountTab({ choices: [FRONT, NONE] })
      await settle()
      await tick()
      expect(root.querySelector('.choice img')?.getAttribute('src')).toBe('data:image/png;base64,small')
      expect(state.coverThumbnail).toHaveBeenCalledWith('k1')
      root.querySelector<HTMLButtonElement>('.choice')?.click()
      await settle()
      await tick()
      // The large view shows the full picture, not the tile's thumbnail.
      expect(state.coverImage).toHaveBeenCalledWith('k1')
      expect(document.querySelector('[role="dialog"] img')?.getAttribute('src')).toBe(
        'data:image/png;base64,iVBORw0KGgo=',
      )
      inDialog('Use for all files').click()
      await settle()
      expect(state.useCoverForAll).toHaveBeenCalledWith('k1')
      expect(document.querySelector('[role="dialog"]')).toBeNull()
    })

    it('while the cover is open large, the window keys stay out (it is a dialog)', async () => {
      const { root } = await mountTab({ choices: [FRONT, NONE] })
      await settle()
      await tick()
      root.querySelector<HTMLButtonElement>('.choice')?.click()
      await tick()
      expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1)
      await bar(root, 'Replace')
      expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1)
    })

    it('is reachable by keyboard: ⇧Tab from the Tracks pane passes the Tracks sort headers, then reaches it before the Album rows', async () => {
      const { root } = await mountTab({ choices: [FRONT, NONE] })
      await settle()
      await tick()
      const tracks = paneList(root, 'Tracks')
      const shiftTab = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })
      tracks.dispatchEvent(shiftTab)
      expect(shiftTab.defaultPrevented).toBe(false)
      // The browser's order: every tab stop, in document order.
      const stops = [
        ...root.querySelectorAll<HTMLElement>('[tabindex="0"], button:not([disabled]):not([tabindex="-1"])'),
      ]
      const at = (element: Element | null) => stops.findIndex((stop) => stop === element)
      const section = root.querySelector('section.cover')
      const album = paneList(root, 'Album tags')
      expect(at(section)).toBeGreaterThan(at(album))
      expect(at(section)).toBeLessThan(at(tracks))
      const between = stops.slice(at(section), at(tracks))
      // The Tracks pane's own sort headers sit just before its list.
      expect(
        between.every(
          (stop) => stop.closest('[data-pane="album"]') !== null || stop.closest('.tag-pane-header') !== null,
        ),
      ).toBe(true)
    })

    it('a pasted picture goes to the state as bytes', async () => {
      const { root, state } = await mountTab({ choices: [NONE] })
      root.querySelector<HTMLElement>('section.cover')?.focus()
      const event = new Event('paste', { bubbles: true, cancelable: true })
      const file = new File([new Uint8Array([0xff, 0xd8, 0xff])], 'p.jpg', { type: 'image/jpeg' })
      Object.defineProperty(event, 'clipboardData', {
        value: { items: [{ kind: 'file', type: 'image/jpeg', getAsFile: () => file }] },
      })
      document.activeElement?.dispatchEvent(event)
      await settle()
      expect(state.coverFromBytes).toHaveBeenCalledWith(new Uint8Array([0xff, 0xd8, 0xff]))
    })
  })

  it('opens with the Album pane focused, and Tab moves focus to the Tracks pane and back', async () => {
    const { root } = await mountTab()
    const album = paneList(root, 'Album tags')
    const tracks = paneList(root, 'Tracks')
    expect(document.activeElement).toBe(album)
    await press(album, 'Tab')
    expect(document.activeElement).toBe(tracks)
    expect(root.querySelector('[data-pane="tracks"] .full-list-container')?.classList.contains('is-focused')).toBe(true)
    await press(tracks, 'Tab')
    expect(document.activeElement).toBe(album)
  })

  it('a pane draws its cursor as focused only while focus is in it, not while it is on the cover', async () => {
    const { root } = await mountTab()
    const albumPane = () => root.querySelector('[data-pane="album"] .full-list-container')
    expect(albumPane()?.classList.contains('is-focused')).toBe(true)
    const cover = [...root.querySelectorAll('button')].find((b) => b.textContent.includes('From folder'))
    cover?.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    await tick()
    expect(albumPane()?.classList.contains('is-focused')).toBe(false)
  })

  it('a tool waits for an edit in flight, then runs', async () => {
    const { root, state } = await mountTab()
    await bar(root, 'Clean up', true)
    inDialog('Strip track prefix').click()
    await settle()
    expect(state.settled).toHaveBeenCalled()
    expect(state.tool).toHaveBeenCalledWith({ kind: 'stripTrackPrefix' }, null)
  })

  it('with 2 of 4 tracks selected, Renumber works on those two only', async () => {
    const tracks = ['01', '02', '03', '04'].map((n) => ({
      ...VIEW.tracks[0],
      path: `/b/${n}.mp3`,
      fileName: `${n}.mp3`,
    })) as TagEditorView['tracks']
    const { root, state } = await mountTab({ view: { ...VIEW, tracks } })
    await toggleTrack(root, '/b/02.mp3')
    await toggleTrack(root, '/b/04.mp3')
    await bar(root, 'Clean up', true)
    inDialog('Renumber').click()
    await settle()
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, ['/b/02.mp3', '/b/04.mp3'])
  })

  it('Delete in the Album pane empties the field on every file, or on the selected tracks alone, without asking', async () => {
    const { root, state } = await mountTab()
    await press(paneList(root, 'Album tags'), 'Delete')
    await settle()
    expect(state.edit).toHaveBeenLastCalledWith([{ kind: 'setAll', field: 'album', values: [] }])
    await toggleTrack(root, '/b/02.mp3')
    await press(paneList(root, 'Album tags'), 'Backspace')
    await settle()
    expect(state.edit).toHaveBeenLastCalledWith([{ kind: 'setOne', path: '/b/02.mp3', field: 'album', values: [] }])
    expect(document.querySelector('[role="dialog"]')).toBeNull()
  })

  it('Delete in the Tracks pane empties the cursor row’s Title; typing there opens the editor', async () => {
    const { root, state } = await mountTab()
    const tracks = paneList(root, 'Tracks')
    await press(tracks, 'ArrowDown')
    await press(tracks, 'Delete')
    await settle()
    expect(state.edit).toHaveBeenLastCalledWith([{ kind: 'setOne', path: '/b/02.mp3', field: 'title', values: [] }])
    await press(tracks, 'K')
    expect(root.querySelector<HTMLInputElement>('.tag-value-editor')?.value).toBe('K')
  })

  it('a view reload drops the selected tracks that are gone for good', async () => {
    const state = $state({
      view: VIEW,
      error: null,
      busy: false,
      open: vi.fn(() => Promise.resolve()),
      edit: vi.fn(() => Promise.resolve()),
      coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
      coverImage: vi.fn(() => Promise.resolve(null)),
      coverChoices: vi.fn(() => Promise.resolve([])),
      coverThumbnail: vi.fn(() => Promise.resolve(null)),
      undoable: null,
      settled: vi.fn(() => Promise.resolve()),
      tool: vi.fn(() => Promise.resolve()),
    })
    const mounted = await mountInto(TagsContent, { state: asTagState(state) })
    cleanup = mounted.cleanup
    const second = () => mounted.root.querySelector<HTMLElement>('[data-track-path="/b/02.mp3"]')
    await toggleTrack(mounted.root, '/b/02.mp3')
    expect(second()?.getAttribute('aria-selected')).toBe('true')

    state.view = { ...VIEW, tracks: VIEW.tracks.slice(0, 1) }
    await tick()
    expect(second()).toBeNull()

    state.view = VIEW
    await tick()
    expect(second()?.getAttribute('aria-selected')).toBe('false')
  })

  it('when every selected track vanished, the tool does not run and a notice says so; the next run takes all', async () => {
    const state = $state({
      view: VIEW,
      error: null,
      busy: false,
      open: vi.fn(() => Promise.resolve()),
      edit: vi.fn(() => Promise.resolve()),
      coverCandidates: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
      coverImage: vi.fn(() => Promise.resolve(null)),
      coverChoices: vi.fn(() => Promise.resolve([])),
      coverThumbnail: vi.fn(() => Promise.resolve(null)),
      undoable: null,
      settled: vi.fn(() => Promise.resolve()),
      tool: vi.fn(() => Promise.resolve()),
    })
    const mounted = await mountInto(TagsContent, { state: asTagState(state) })
    cleanup = mounted.cleanup
    const renumber = async () => {
      await bar(mounted.root, 'Clean up', true)
      inDialog('Renumber').click()
    }
    await toggleTrack(mounted.root, '/b/02.mp3')
    const renamed = VIEW.tracks.map((t) => ({ ...t, path: `${t.path}.new` }))
    state.view = { ...VIEW, tracks: renamed }
    await tick()

    await renumber()
    await settle()
    expect(state.tool).not.toHaveBeenCalled()
    expect(mounted.root.textContent).toContain('The selected tracks are no longer here, so nothing was changed.')

    await renumber()
    await settle()
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, null)
    expect(mounted.root.textContent).not.toContain('no longer here')
  })

  it('the Find and replace panel works on the selected tracks alone', async () => {
    const { root, state } = await mountTab()
    await toggleTrack(root, '/b/02.mp3')
    await bar(root, 'Replace')
    await typeInto(byId<HTMLInputElement>(document.body, 'tag-replace-search'), 'a')
    inDialog('Replace').click()
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

  it('Case changes the Album pane’s cursor field while that pane has focus', async () => {
    const { root, state } = await mountTab()
    paneList(root, 'Album tags').dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    await bar(root, 'Case', true)
    inDialog('lower case').click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({ kind: 'case', case: 'lower', fields: ['album'] }, null)
  })

  it('Case changes Title while the Tracks pane has focus', async () => {
    const { root, state } = await mountTab()
    paneList(root, 'Tracks').dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    await bar(root, 'Case', true)
    inDialog('lower case').click()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(state.tool).toHaveBeenCalledWith({ kind: 'case', case: 'lower', fields: ['title'] }, null)
  })

  it('Case takes the field the user was on when it opened; on the cover it takes the five name fields', async () => {
    const { root, state } = await mountTab()
    const focusIn = (el: Element) => el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    focusIn(paneList(root, 'Album tags'))
    await bar(root, 'Case', true)
    expect(document.body.textContent).toContain('Changes: Album')
    inDialog('Close').click()
    await settle()
    const cover = [...root.querySelectorAll('button')].find((b) => b.textContent.includes('From folder'))
    if (!cover) throw new Error('no cover button')
    focusIn(cover)
    await bar(root, 'Case', true)
    inDialog('lower case').click()
    await settle()
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

  it('read-only says why, opens no editor, and locks every button that writes', async () => {
    const { root } = await mountTab({ view: { ...VIEW, editable: false, readOnlyReason: 'remoteVolume' } })
    // Once, in the status line: it explains every locked row and key, Write included.
    expect(root.textContent.split('Tags can only be edited for files on a local disk.')).toHaveLength(2)
    await press(paneList(root, 'Album tags'), 'F2')
    await press(paneList(root, 'Tracks'), 'F2')
    expect(root.querySelector('.tag-value-editor')).toBeNull()
    // Playing, selecting and sorting write nothing, so Play, Open in foobar2000, the select-all box and the column
    // headers stay available.
    const readers = new Set(['Open in foobar2000', 'F3Play'])
    const writers = [...root.querySelectorAll('button')].filter(
      (b) => !readers.has(b.textContent.trim()) && !b.classList.contains('sortable-header'),
    )
    for (const button of writers) expect(button.disabled).toBe(true)
  })
})
