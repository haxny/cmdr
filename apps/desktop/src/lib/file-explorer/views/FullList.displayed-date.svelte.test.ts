/**
 * The Full view's date column shows the date its rows are sorted by
 * (`displayedDateColumn`): cells, header label, the fallback glyph, the row's
 * accessible name, and the cell's tooltip all follow it. Retained rows keep the
 * date of the sort they were fetched under until their replacement lands.
 */

import { describe, expect, it, vi } from 'vitest'
import { tick, type ComponentProps } from 'svelte'
import { getFileRange } from '#lib/tauri-commands/index.js'
import { fileEntry, mountFullList } from './test-full-list'
import type FullList from './FullList.svelte'

vi.mock('#lib/tauri-commands/index.js', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({
    // The timestamp itself, so each cell says which date it shows.
    formattedDate: (t: number | null | undefined) =>
      t != null ? { text: String(t), segments: [{ text: String(t), ageClass: null }] } : { text: '', segments: [] },
  }),
)
vi.mock('#lib/settings/settings-store.js', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

type Props = ComponentProps<typeof FullList>

const MODIFIED = 1_000
const ADDED = 2_000
const entries = [
  fileEntry({ name: 'a.txt', modifiedAt: MODIFIED, addedAt: ADDED }),
  fileEntry({ name: 'b.txt', modifiedAt: MODIFIED, addedAt: ADDED }),
]

function dateCells(target: HTMLElement): string[] {
  return [...target.querySelectorAll('.file-entry .col-date')].map((cell) => cell.textContent.trim())
}

function dateHeader(target: HTMLElement): HTMLElement {
  const buttons = [...target.querySelectorAll<HTMLElement>('.header-row .sortable-header')]
  const header = buttons.at(-1)
  if (!header) throw new Error('no date header rendered')
  return header
}

describe('the Full date column', () => {
  it('shows Added cells and the Added label under an Added sort', async () => {
    const list = await mountFullList({ entries, props: { sortBy: 'added', effectiveSortBy: 'added' } })

    expect(dateCells(list.target)).toEqual([String(ADDED), String(ADDED)])
    expect(dateHeader(list.target).querySelector('.label')?.textContent).toBe('Added')
    expect(dateHeader(list.target).classList.contains('is-active')).toBe(true)
    expect(dateHeader(list.target).querySelector('.status-glyph')).toBeNull()
    expect(list.rows()[0].getAttribute('aria-label')).toContain(`(added) ${String(ADDED)}`)
  })

  it('shows Modified with the fallback glyph when an Added sort fell back', async () => {
    const list = await mountFullList({ entries, props: { sortBy: 'added', effectiveSortBy: 'modified' } })

    expect(dateCells(list.target)).toEqual([String(MODIFIED), String(MODIFIED)])
    expect(dateHeader(list.target).querySelector('.label')?.textContent).toBe('Modified')
    expect(dateHeader(list.target).classList.contains('is-active')).toBe(true)
    expect(dateHeader(list.target).querySelector('.status-glyph')).not.toBeNull()
    expect(list.rows()[0].getAttribute('aria-label')).toContain(`(modified) ${String(MODIFIED)}`)
  })

  it('shows Modified under a Name sort, with no glyph', async () => {
    const list = await mountFullList({ entries, props: { sortBy: 'name', effectiveSortBy: 'name' } })

    expect(dateCells(list.target)).toEqual([String(MODIFIED), String(MODIFIED)])
    expect(dateHeader(list.target).classList.contains('is-active')).toBe(false)
    expect(dateHeader(list.target).querySelector('.status-glyph')).toBeNull()
  })

  // The pane takes the new folder's effective column at `listing-complete`, one fetch
  // before the new rows replace the old ones here. Reading it live would paint the old
  // folder's rows with the new folder's date for that fetch.
  it('keeps the date its retained rows were fetched under until the new rows land', async () => {
    const live = $state<Partial<Props>>({})
    const list = await mountFullList({
      entries,
      liveProps: live,
      props: { sortBy: 'added', effectiveSortBy: 'modified' },
    })
    expect(dateCells(list.target)).toEqual([String(MODIFIED), String(MODIFIED)])

    let release: (rows: ReturnType<typeof fileEntry>[]) => void = () => {}
    vi.mocked(getFileRange).mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve
        }),
    )
    live.listingId = 'listing-2'
    live.effectiveSortBy = 'added'
    await tick()

    expect(list.rowNames()).toEqual(['a.txt', 'b.txt'])
    expect(dateCells(list.target)).toEqual([String(MODIFIED), String(MODIFIED)])
    expect(dateHeader(list.target).querySelector('.label')?.textContent).toBe('Modified')

    release([fileEntry({ name: 'new.txt', modifiedAt: MODIFIED, addedAt: ADDED })])
    await list.settle(() => list.rowNames().includes('new.txt'), 'the replacement rows')

    expect(dateCells(list.target)).toEqual([String(ADDED)])
    expect(dateHeader(list.target).querySelector('.label')?.textContent).toBe('Added')
  })

  it('lists every date in the cell tooltip and marks the shown one', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const list = await mountFullList({ entries, props: { sortBy: 'added', effectiveSortBy: 'added' } })
      const cell = list.target.querySelector<HTMLElement>('.file-entry .col-date')
      cell?.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      const tip = document.querySelector('.cmdr-tooltip.visible')?.textContent ?? ''
      expect(tip).toBe(`Date added (displayed): ${String(ADDED)}Last modified: ${String(MODIFIED)}`)
      cell?.dispatchEvent(new MouseEvent('mouseleave'))
    } finally {
      vi.useRealTimers()
    }
  })
})
