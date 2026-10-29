/**
 * The Tracks pane under its filter (⌘F): it lists the shown rows alone, with their indices in the whole list, so the
 * cursor steps over hidden rows and the selection stays on paths; the field is labelled, its count is a status, and
 * Esc (in the field or on the list) clears and closes it.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { tick } from 'svelte'

vi.hoisted(() => {
  vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
})

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getRowHeight: () => 23,
  getStripedRows: () => true,
}))

import type { TrackRow } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TracksPane from './TracksPane.svelte'
import { createTrackSelection } from './track-selection.svelte'
import { shownTrackIndices } from './track-filter'
import { mountInto, press, typeInto } from './tag-cell.test-helper'

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

function track(n: string, disc: string): TrackRow {
  return {
    path: `/b/${n}.mp3`,
    fileName: `${n}.mp3`,
    track: n,
    disc,
    title: `Část ${n}`,
    durationMs: 61_000,
    changed: false,
    readError: null,
  }
}

const TRACKS = [track('01', '1'), track('02', '2'), track('03', '1'), track('04', '2')]

/** The pane as `TagsContent` drives it: the filter text in state, the shown rows derived from it. */
async function mountFiltered(text: string | null) {
  const selection = createTrackSelection()
  selection.sync(TRACKS.map((t) => t.path))
  const props = $state({
    filter: text,
    shown: shownTrackIndices(TRACKS, text),
    cursor: 0,
  })
  const onFilterChange = vi.fn((next: string | null) => {
    props.filter = next
    props.shown = shownTrackIndices(TRACKS, next)
    selection.setShown(props.shown === null ? null : props.shown.map((index) => TRACKS[index].path))
  })
  selection.setShown(props.shown === null ? null : props.shown.map((index) => TRACKS[index].path))
  const mounted = await mountInto(TracksPane, {
    tracks: TRACKS,
    editable: true,
    focused: true,
    get cursor() {
      return props.cursor
    },
    onCursorChange: (index: number) => {
      props.cursor = index
    },
    selection,
    playing: null,
    onPlay: vi.fn(),
    onEdit: vi.fn(),
    onClear: vi.fn(),
    onSwitchPane: vi.fn(),
    sort: null,
    onSort: vi.fn(),
    get filter() {
      return props.filter
    },
    get shown() {
      return props.shown
    },
    onFilterChange,
  })
  cleanup = mounted.cleanup
  const list = mounted.root.querySelector<HTMLElement>('[role="listbox"]')
  if (!list) throw new Error('no listbox')
  return { root: mounted.root, list, selection, props, onFilterChange }
}

const rowPaths = (root: HTMLElement) =>
  [...root.querySelectorAll<HTMLElement>('[role="option"]')].map((row) => row.dataset.trackPath)
const fieldOf = (root: HTMLElement) => root.querySelector<HTMLInputElement>('.filter-bar input')

describe('TracksPane filter', () => {
  it('lists the shown rows alone, each keeping its index in the whole list', async () => {
    const { root } = await mountFiltered('disc: 2')
    expect(rowPaths(root)).toEqual(['/b/02.mp3', '/b/04.mp3'])
    const indices = [...root.querySelectorAll<HTMLElement>('[role="option"]')].map((row) => row.dataset.index)
    expect(indices).toEqual(['1', '3'])
  })

  it('typing in the field narrows the list, folding case and diacritics', async () => {
    const { root, onFilterChange } = await mountFiltered('')
    const field = fieldOf(root)
    if (!field) throw new Error('no filter field')
    await typeInto(field, 'cast 03')
    expect(onFilterChange).toHaveBeenLastCalledWith('cast 03')
    expect(rowPaths(root)).toEqual(['/b/03.mp3'])
  })

  it('↓ steps over hidden rows, and ⇧↓ selects the shown rows alone', async () => {
    const { list, props, selection } = await mountFiltered('disc: 2')
    props.cursor = 1
    await tick()
    await press(list, 'ArrowDown')
    expect(props.cursor).toBe(3)
    props.cursor = 1
    await tick()
    await press(list, 'ArrowDown', { shiftKey: true })
    expect([...selection.selected]).toEqual(['/b/02.mp3'])
    await press(list, 'ArrowDown', { shiftKey: true })
    expect([...selection.selected].sort()).toEqual(['/b/02.mp3', '/b/04.mp3'])
  })

  it('the selection survives a filter change; the count says how many selected tracks it hides', async () => {
    const { root, selection, onFilterChange } = await mountFiltered(null)
    selection.toggleAt(0)
    selection.toggleAt(1)
    onFilterChange('disc: 2')
    await tick()
    const status = root.querySelector('.filter-bar [role="status"]')
    expect(status?.textContent).toContain('2 of 4 tracks')
    expect(status?.textContent).toContain('1 selected track hidden')
    onFilterChange(null)
    await tick()
    expect([...selection.selected].sort()).toEqual(['/b/01.mp3', '/b/02.mp3'])
  })

  it('Esc in the field clears and closes the filter, and the window never sees it', async () => {
    const { root, onFilterChange } = await mountFiltered('disc')
    const field = fieldOf(root)
    if (!field) throw new Error('no filter field')
    const esc = await press(field, 'Escape')
    expect(esc.defaultPrevented).toBe(true)
    expect(onFilterChange).toHaveBeenLastCalledWith(null)
    expect(fieldOf(root)).toBeNull()
    expect(rowPaths(root)).toHaveLength(4)
  })

  it('Esc on the list clears the filter while it is on, even when it shows nothing', async () => {
    const { list, onFilterChange } = await mountFiltered('nothing like it')
    const esc = await press(list, 'Escape')
    expect(esc.defaultPrevented).toBe(true)
    expect(onFilterChange).toHaveBeenLastCalledWith(null)
  })

  it('Esc on the list without a filter is left to the window (it closes)', async () => {
    const { list, onFilterChange } = await mountFiltered(null)
    const esc = await press(list, 'Escape')
    expect(esc.defaultPrevented).toBe(false)
    expect(onFilterChange).not.toHaveBeenCalled()
  })

  it('the field is labelled and the filtered pane has no a11y violations', async () => {
    const { root } = await mountFiltered('disc: 2')
    expect(fieldOf(root)?.getAttribute('aria-label')).toBe('Filter the tracks')
    await expectNoA11yViolations(root)
  })
})
