/**
 * The scroll position the browser moves on its own.
 *
 * When the listing shrinks (a quick filter narrowing it, a delete), the spacer
 * gets shorter and WebKit clamps the scroller's `scrollTop` down WITHOUT firing a
 * `scroll` event. A list that only learns its offset from scroll events keeps the
 * old, larger one, so its virtual window starts past the rows that exist and the
 * pane renders blank, and stays blank (or offset) after the filter clears, until
 * the next real scroll. Regression anchor for ERR-YFPVF (0.53.0).
 *
 * jsdom never clamps, so each spec plays the browser: it writes the clamped
 * `scrollTop` itself and fires no event.
 */

import { describe, it, expect, vi } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'

vi.mock('#lib/tauri-commands/index.js', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('#lib/settings/settings-store.js', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

const SURFACE = '[data-file-list-surface]'
// The mocks' row height; the default viewport shows 20 rows, with 20 more buffered each side.
const ROW_HEIGHT = 20

const ALL = Array.from({ length: 100 }, (_, i) => fileEntry({ name: `file-${String(i).padStart(3, '0')}.txt` }))
const NARROWED = [ALL[3], ALL[30], ALL[70]]

/** Swaps what the mocked backend serves, the way a filter swaps the listing's row space. */
function serve(entries: typeof ALL, next: typeof ALL): void {
  entries.splice(0, entries.length, ...next)
}

/** The browser's silent clamp: the new `scrollTop`, and no `scroll` event. */
function clampSilently(target: HTMLElement, scrollTop: number): void {
  const surface = target.querySelector<HTMLElement>(SURFACE)
  if (!surface) throw new Error('no scroll surface mounted')
  surface.scrollTop = scrollTop
}

describe('FullList when the browser clamps the scroll position', () => {
  it('renders the narrowed rows after the listing shrinks below the scrolled-to offset', async () => {
    const entries = [...ALL]
    const props = $state({}) as Record<string, unknown>
    const list = await mountFullList({ entries, liveProps: props })

    list.layout.scroll(SURFACE, 60 * ROW_HEIGHT)
    await list.settle(() => list.rowNames().includes('file-060.txt'), 'row 60 to render after the scroll')

    serve(entries, NARROWED)
    props.totalCount = NARROWED.length
    props.cacheGeneration = 1
    clampSilently(list.target, 0)

    await list.settle(() => list.rowNames().includes('file-003.txt'), 'the narrowed rows to render')
    expect(list.rowNames()).toEqual(['file-003.txt', 'file-030.txt', 'file-070.txt'])
  })

  it('renders from the top again once the filter clears', async () => {
    const entries = [...ALL]
    const props = $state({}) as Record<string, unknown>
    const list = await mountFullList({ entries, liveProps: props })

    list.layout.scroll(SURFACE, 60 * ROW_HEIGHT)
    await list.settle(() => list.rowNames().includes('file-060.txt'), 'row 60 to render after the scroll')

    serve(entries, NARROWED)
    props.totalCount = NARROWED.length
    props.cacheGeneration = 1
    clampSilently(list.target, 0)
    await list.settle(() => list.rowNames().length === NARROWED.length, 'the narrowed rows to render')

    serve(entries, ALL)
    props.totalCount = ALL.length
    props.cacheGeneration = 2

    await list.settle(() => list.rowNames().includes('file-000.txt'), 'row 0 to render at the top')
    expect(list.rowNames()[0]).toBe('file-000.txt')
  })
})
