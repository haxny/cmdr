/** Tier 3 a11y tests for `TracksPane.svelte`: tracks with a selection, one playing, an unreadable one, one file, an open editor. */
import { afterEach, describe, it, vi } from 'vitest'
import type { TrackRow, TrackSort } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TracksPane from './TracksPane.svelte'
import { createTrackSelection } from './track-selection.svelte'
import { mountInto, press } from './tag-cell.test-helper'

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getRowHeight: () => 20,
  getStripedRows: () => true,
}))

let cleanup: (() => void) | undefined
afterEach(() => {
  cleanup?.()
  cleanup = undefined
})

const TRACKS: TrackRow[] = [
  {
    path: '/b/01.mp3',
    fileName: '01.mp3',
    track: '1',
    title: 'Prvni',
    durationMs: 61_000,
    changed: true,
    readError: null,
  },
  {
    path: '/b/02.mp3',
    fileName: '02.mp3',
    track: null,
    title: null,
    durationMs: null,
    changed: false,
    readError: 'unreadable',
  },
]

async function mountPane(tracks: TrackRow[], editable = true, sort: TrackSort | null = null): Promise<HTMLElement> {
  const selection = createTrackSelection()
  selection.sync(tracks.map((t) => t.path))
  selection.toggleAt(0)
  const mounted = await mountInto(TracksPane, {
    tracks,
    editable,
    focused: true,
    cursor: 0,
    onCursorChange: () => {},
    selection,
    playing: '/b/01.mp3',
    onPlay: () => {},
    onEdit: () => {},
    onSwitchPane: () => {},
    sort,
    onSort: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('TracksPane a11y', () => {
  it('with tracks, a selection, and one playing has no violations', async () => {
    await expectNoA11yViolations(await mountPane(TRACKS))
  })

  it('a single file has no violations', async () => {
    await expectNoA11yViolations(await mountPane(TRACKS.slice(0, 1)))
  })

  it('with the Title editor open has no violations', async () => {
    const root = await mountPane(TRACKS)
    const list = root.querySelector<HTMLElement>('[role="listbox"]')
    if (!list) throw new Error('no listbox')
    await press(list, 'F2')
    await expectNoA11yViolations(root)
  })

  it('sorted by a column, with the direction caret, has no violations', async () => {
    await expectNoA11yViolations(await mountPane(TRACKS, true, { key: 'title', descending: true }))
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountPane(TRACKS, false))
  })
})
