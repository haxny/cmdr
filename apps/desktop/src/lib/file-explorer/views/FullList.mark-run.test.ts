/**
 * `FullList` puts the marked-file bar's run edges on each row (`mark-run.ts`): the top end
 * rounds on a run's first row, the bottom end on its last.
 */

import { describe, expect, it, vi } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'

vi.mock('#lib/tauri-commands/index.js', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('#lib/settings/settings-store.js', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

function edgesOf(rows: HTMLElement[], filename: string): string[] {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  return ['is-mark-run-start', 'is-mark-run-end'].filter((c) => row.classList.contains(c))
}

describe('FullList marked-run edges', () => {
  it('rounds the outer ends of each run and leaves its middle open', async () => {
    const entries = ['a', 'b', 'c', 'd', 'e'].map((name) => fileEntry({ name }))
    const list = await mountFullList({ entries, props: { cursorIndex: -1, selectedIndices: new Set([0, 1, 2, 4]) } })
    const rows = list.rows()

    expect(edgesOf(rows, 'a')).toEqual(['is-mark-run-start'])
    expect(edgesOf(rows, 'b')).toEqual([])
    expect(edgesOf(rows, 'c')).toEqual(['is-mark-run-end'])
    expect(edgesOf(rows, 'd')).toEqual([])
    expect(edgesOf(rows, 'e')).toEqual(['is-mark-run-start', 'is-mark-run-end'])
  })
})
