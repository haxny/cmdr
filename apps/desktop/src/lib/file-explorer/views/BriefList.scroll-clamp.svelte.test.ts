/**
 * `BriefList`'s twin of `FullList.scroll-clamp.svelte.test.ts`: when the listing
 * shrinks, WebKit clamps `scrollLeft` with no `scroll` event, and a list that only
 * learns its offset from scroll events renders a window past the columns that
 * exist (a blank pane). Regression anchor for ERR-YFPVF (0.53.0).
 *
 * Brief stays correct without a dedicated resync: its window math clamps to the
 * last column, and its keep-cursor-in-view effect re-runs on the new column widths
 * and writes `scrollLeft` back through `scrollToIndex`. This pins that, so a
 * refactor of either can't quietly reopen the bug Full had.
 *
 * jsdom never clamps, so each spec plays the browser: it writes the clamped
 * `scrollLeft` itself and fires no event.
 */

import { describe, expect, it, vi } from 'vitest'
import { type ComponentProps } from 'svelte'
import { fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'
import type BriefList from './BriefList.svelte'

vi.mock('#lib/tauri-commands/index.js', async () =>
  (await import('./test-file-list-mocks')).tauriCommandsMock({
    getBriefColumnTextWidths: () => Promise.resolve({ status: 'ok', data: { widths: [], missingCodePoints: [] } }),
  }),
)
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({
    getBriefColumnWidthMode: () => 'paneWidth',
    getBriefColumnWidthMaxPx: () => 400,
  }),
)
vi.mock('#lib/settings/settings-store.js', async () =>
  (await import('./test-file-list-mocks')).settingsStoreMock({
    'advanced.virtualizationBufferColumns': 2,
  }),
)

type Props = ComponentProps<typeof BriefList>

const SURFACE = '[data-file-list-surface]'
const ALL = Array.from({ length: 1000 }, (_, i) => fileEntry({ name: `file-${String(i).padStart(4, '0')}.txt` }))
const NARROWED = [ALL[3], ALL[300], ALL[700]]

function surfaceOf(target: HTMLElement): HTMLElement {
  const surface = target.querySelector<HTMLElement>(SURFACE)
  if (!surface) throw new Error('no scroll surface mounted')
  return surface
}

describe('BriefList when the browser clamps the scroll position', () => {
  it('renders the narrowed rows after the listing shrinks below the scrolled-to offset', async () => {
    const entries = [...ALL]
    const live = $state<Partial<Props>>({})
    const list = await mountBriefList({ entries, liveProps: live })

    const surface = surfaceOf(list.target)
    surface.scrollLeft = 20_000
    surface.dispatchEvent(new Event('scroll'))
    await list.settle(() => !list.rowNames().includes('file-0000.txt'), 'the first column to scroll out of the window')

    entries.splice(0, entries.length, ...NARROWED)
    live.totalCount = NARROWED.length
    live.cacheGeneration = 1
    // The browser's silent clamp: the new offset, and no `scroll` event.
    surface.scrollLeft = 0

    await list.settle(() => list.rowNames().includes('file-0003.txt'), 'the narrowed rows to render')
    expect(list.rowNames()).toEqual(['file-0003.txt', 'file-0300.txt', 'file-0700.txt'])

    // Clearing the filter: the columns come back, and the viewport is at the start.
    entries.splice(0, entries.length, ...ALL)
    live.totalCount = ALL.length
    live.cacheGeneration = 2

    await list.settle(() => list.rowNames().includes('file-0000.txt'), 'the first column to render again')
    expect(list.rowNames()[0]).toBe('file-0000.txt')
  })
})
