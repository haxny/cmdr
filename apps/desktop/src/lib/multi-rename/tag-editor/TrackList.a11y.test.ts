/** Tier 3 a11y tests for `TrackList.svelte`: several tracks (with an unreadable one), a single file, and locked. */
import { afterEach, describe, it } from 'vitest'
import type { TrackRow } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import TrackList from './TrackList.svelte'
import { mountInto } from './tag-cell.test-helper'

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

async function mountList(tracks: TrackRow[], editable = true): Promise<HTMLElement> {
  const mounted = await mountInto(TrackList, {
    tracks,
    editable,
    selected: new Set(['/b/01.mp3']),
    onEdit: () => {},
    onToggle: () => {},
    onToggleAll: () => {},
    playing: '/b/01.mp3',
    onPlay: () => {},
  })
  cleanup = mounted.cleanup
  return mounted.root
}

describe('TrackList a11y', () => {
  it('with tracks has no violations', async () => {
    await expectNoA11yViolations(await mountList(TRACKS))
  })

  it('a single file has no violations', async () => {
    await expectNoA11yViolations(await mountList(TRACKS.slice(0, 1)))
  })

  it('read-only has no violations', async () => {
    await expectNoA11yViolations(await mountList(TRACKS, false))
  })
})
