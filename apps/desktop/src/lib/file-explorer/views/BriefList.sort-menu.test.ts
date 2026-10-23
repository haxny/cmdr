/**
 * Brief's sort header opens the same date Sort by popup as Full's on a right-click,
 * so the two views behave alike. (Only Full has the chevron: Brief shows a button per date.)
 */

import { describe, expect, it, vi } from 'vitest'
import { fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'
import type { MenuAnchor } from '#lib/tauri-commands/index.js'

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

describe('BriefList sort header right-click', () => {
  it('opens the date Sort by popup at the pointer, and no page menu', async () => {
    const opens: (MenuAnchor | null)[] = []
    const list = await mountBriefList({
      entries: [fileEntry({ name: 'a.txt' })],
      props: {
        onSortMenu: (anchor: MenuAnchor | null) => {
          opens.push(anchor)
          return Promise.resolve()
        },
      },
    })
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })

    list.target.querySelector('.header-row .sortable-header')?.dispatchEvent(event)

    expect(opens).toEqual([null])
    expect(event.defaultPrevented).toBe(true)
  })
})
