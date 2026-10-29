/**
 * The Tags window under the Tracks filter (⌘F): with nothing selected the tools, the Album pane's values and its edits
 * work on the tracks the filter shows, named by path; a selection wins (hidden selected tracks included); ⌘A selects
 * the shown tracks; a filter that shows nothing changes nothing.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import TagsContent from './TagsContent.svelte'
import type { TagEditorState } from './tag-editor-state.svelte'
import { editCursorRow, paneList, press, toggleTrack, typeInto } from './tag-cell.test-helper'

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

/** Ten tracks: 01–05 on disc 1, 06–10 on disc 2. */
const TEN = Array.from({ length: 10 }, (_, index) => {
  const n = String(index + 1).padStart(2, '0')
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
    disc: index < 5 ? '1' : '2',
    title: `Kapitola ${n}`,
    durationMs: 1000,
    changed: false,
    readError: null,
  }
}) satisfies TagEditorView['tracks']
const DISC_TWO = TEN.slice(5).map((track) => track.path)

const VIEW: TagEditorView = {
  editable: true,
  readOnlyReason: null,
  shared: [{ field: 'album', value: { same: ['Kniha'] }, choices: [], choicesMore: 0 }],
  tracks: TEN,
  cover: { kind: 'empty' },
  extras: [],
  dirty: false,
  diacritics: [],
  badDates: 0,
  canUndoEdit: false,
  canRedoEdit: false,
  scopeCount: null,
  viewSeq: 0,
  sort: null,
}

/** How long the scope must rest before the Album pane follows it (`VIEW_SCOPE_DELAY_MS`), and a bit. */
const scopeRest = () => new Promise((resolve) => setTimeout(resolve, 200))
const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

interface Content {
  runCommand: (id: 'tags.filter') => boolean
  selectAllTracks: () => void
}

/** A state whose `setViewScope` answers as the backend does: the count of the scope, or `null` for all. */
async function mountFiltered() {
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
    setViewScope: vi.fn((paths: string[] | null) => {
      state.view = { ...state.view, scopeCount: paths === null ? null : paths.length }
      return Promise.resolve(true)
    }),
  })
  const root = document.createElement('div')
  document.body.appendChild(root)
  const content = mount(TagsContent, { target: root, props: { state: state as unknown as TagEditorState } }) as Content
  cleanup = () => {
    void unmount(content)
    root.remove()
  }
  await tick()
  const header = () => root.querySelector('.tag-pane-header')?.textContent ?? ''
  /** ⌘F, then `text` typed into the field. */
  async function filterBy(text: string): Promise<void> {
    expect(content.runCommand('tags.filter')).toBe(true)
    await settle()
    const field = root.querySelector<HTMLInputElement>('.filter-bar input')
    if (!field) throw new Error('no filter field')
    expect(document.activeElement).toBe(field)
    await typeInto(field, text)
  }
  const rows = () => [...root.querySelectorAll<HTMLElement>('[role="option"][data-track-path]')]
  return { root, state, content, header, filterBy, rows }
}

/** Renumber from the Clean up dialog, which runs on the tools' scope. */
async function renumber(root: HTMLElement): Promise<void> {
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
  await tick()
  const button = [...root.querySelectorAll<HTMLButtonElement>('[role="toolbar"] button')].find(
    (b) => b.querySelector('span')?.textContent === 'Clean up',
  )
  if (!button) throw new Error('no Clean up key')
  button.click()
  document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift' }))
  await settle()
  const run = [...document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')].find(
    (b) => b.textContent.trim() === 'Renumber',
  )
  if (!run) throw new Error('no Renumber')
  run.click()
  await settle()
}

describe('TagsContent with the Tracks filter', () => {
  it('⌘F filters the tracks; the Album pane then sums up the shown ones and says so', async () => {
    const { root, state, header, filterBy, rows } = await mountFiltered()
    await filterBy('disc: 2')
    expect(rows().map((row) => row.dataset.trackPath)).toEqual(DISC_TWO)
    await scopeRest()
    expect(state.setViewScope).toHaveBeenLastCalledWith([...DISC_TWO].sort())
    await tick()
    expect(header()).toContain('5 of 10 tracks (filtered)')
    const list = paneList(root, 'Album tags')
    expect(document.getElementById(list.getAttribute('aria-describedby') ?? '')?.textContent).toBe(
      '5 of 10 tracks (filtered)',
    )
  })

  it('a tool and an Album edit go to the shown tracks alone, passed by path', async () => {
    const { root, state, filterBy } = await mountFiltered()
    await filterBy('disc: 2')
    await renumber(root)
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, DISC_TWO)
    document.querySelector<HTMLButtonElement>('[role="dialog"] button')?.click()
    await settle()
    const album = await editCursorRow(paneList(root, 'Album tags'))
    await typeInto(album, 'Druhý díl')
    await press(album, 'Enter')
    await settle()
    expect(state.edit).toHaveBeenLastCalledWith(
      DISC_TWO.map((path) => ({ kind: 'setOne', path, field: 'album', values: ['Druhý díl'] })),
    )
  })

  it('a selection wins over the filter, its hidden tracks too, and the field says how many are hidden', async () => {
    const { root, state, filterBy } = await mountFiltered()
    await toggleTrack(root, '/b/01.mp3')
    await filterBy('disc: 2')
    expect(root.querySelector('[data-filter-count]')?.textContent).toContain('1 selected track hidden')
    await renumber(root)
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, ['/b/01.mp3'])
  })

  it('⌘A selects the shown tracks, and they stay selected once the filter is cleared', async () => {
    const { root, state, content, filterBy } = await mountFiltered()
    await filterBy('kapitola 0')
    await filterBy('disc: 2')
    content.selectAllTracks()
    await tick()
    const field = root.querySelector<HTMLInputElement>('.filter-bar input')
    if (!field) throw new Error('no filter field')
    await press(field, 'Escape')
    expect(root.querySelector('.filter-bar')).toBeNull()
    const selected = [...root.querySelectorAll<HTMLElement>('[role="option"][data-track-path][aria-selected="true"]')]
    expect(selected.map((row) => row.dataset.trackPath)).toEqual(DISC_TWO)
    await renumber(root)
    expect(state.tool).toHaveBeenCalledWith({ kind: 'renumber', start: 1 }, DISC_TWO)
  })

  it('a filter that shows nothing changes nothing: the tool is refused with a notice', async () => {
    const { root, state, header, filterBy } = await mountFiltered()
    await filterBy('nothing like it')
    expect(header()).toContain('0 of 10 tracks (filtered)')
    await renumber(root)
    expect(state.tool).not.toHaveBeenCalled()
    expect(root.querySelector('.status')?.textContent).toContain('No track matches the filter')
  })
})
