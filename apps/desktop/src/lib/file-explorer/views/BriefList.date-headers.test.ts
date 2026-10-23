/**
 * Brief's header has a button per date, Added after Created, and the requested date's
 * button carries the fallback glyph when the location can't sort by it. The rows stay
 * name-only: a Brief row shows no date, the status bar below does.
 */

import { describe, expect, it, vi } from 'vitest'
import { fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'

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

const entries = [fileEntry({ name: 'a.txt', modifiedAt: 1_000, addedAt: 2_000 })]

function headerButtons(target: HTMLElement): HTMLElement[] {
  return [...target.querySelectorAll<HTMLElement>('.header-row .sortable-header')]
}

function headerLabels(target: HTMLElement): string[] {
  return headerButtons(target).map((button) => button.querySelector('.label')?.textContent ?? '')
}

function glyphLabels(target: HTMLElement): string[] {
  return headerButtons(target)
    .filter((button) => button.querySelector('.status-glyph') !== null)
    .map((button) => button.querySelector('.label')?.textContent ?? '')
}

describe('the Brief sort header', () => {
  it('has a button per sort column, Added last', async () => {
    const list = await mountBriefList({ entries })

    expect(headerLabels(list.target)).toEqual(['Name', 'Ext', 'Size', 'Modified', 'Created', 'Added'])
  })

  it('lights up Added under an Added sort, with no glyph', async () => {
    const list = await mountBriefList({ entries, props: { sortBy: 'added', effectiveSortBy: 'added' } })

    const added = headerButtons(list.target).at(-1)
    expect(added?.classList.contains('is-active')).toBe(true)
    expect(glyphLabels(list.target)).toEqual([])
  })

  it('puts the fallback glyph on the requested Added button when the sort fell back', async () => {
    const list = await mountBriefList({ entries, props: { sortBy: 'added', effectiveSortBy: 'modified' } })

    expect(glyphLabels(list.target)).toEqual(['Added'])
    expect(headerButtons(list.target).at(-1)?.classList.contains('is-active')).toBe(true)
  })

  it('puts it on Created for a fallen-back Created sort', async () => {
    const list = await mountBriefList({ entries, props: { sortBy: 'created', effectiveSortBy: 'modified' } })

    expect(glyphLabels(list.target)).toEqual(['Created'])
  })

  it('keeps the rows name-only', async () => {
    const list = await mountBriefList({ entries, props: { sortBy: 'added', effectiveSortBy: 'added' } })

    expect(list.rows()[0].getAttribute('aria-label')).toBe('a.txt')
  })
})
