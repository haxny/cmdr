/**
 * `BriefList` puts the marked-file bar's run edges on each row (`mark-run.ts`). Column
 * boundaries are covered by `mark-run.test.ts`; this pins the wiring.
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

function edgesOf(rows: HTMLElement[], filename: string): string[] {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  return ['is-mark-run-start', 'is-mark-run-end'].filter((c) => row.classList.contains(c))
}

describe('BriefList marked-run edges', () => {
  it('rounds the outer ends of each run and leaves its middle open', async () => {
    const entries = ['a', 'b', 'c', 'd'].map((name) => fileEntry({ name }))
    const list = await mountBriefList({ entries, props: { cursorIndex: -1, selectedIndices: new Set([0, 1, 3]) } })
    const rows = list.rows()

    expect(edgesOf(rows, 'a')).toEqual(['is-mark-run-start'])
    expect(edgesOf(rows, 'b')).toEqual(['is-mark-run-end'])
    expect(edgesOf(rows, 'c')).toEqual([])
    expect(edgesOf(rows, 'd')).toEqual(['is-mark-run-start', 'is-mark-run-end'])
  })
})
