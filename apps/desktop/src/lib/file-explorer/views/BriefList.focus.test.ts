/**
 * `BriefList`'s focus model for screen readers (`test-list-focus.svelte.ts` runs the cases
 * shared with `FullList`, which runs them from `full-list.a11y.test.ts`).
 *
 * Its own file: `BriefList.a11y.test.ts` stubs the IPC module without the column-width
 * command a populated, measured list asks for, and `mountBriefList` needs it.
 */

import { afterEach, describe, vi } from 'vitest'
import { mountBriefList } from './test-brief-list'
import { listFocusCases } from './test-list-focus.svelte'

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

afterEach(() => {
  document.body.innerHTML = ''
})

describe('BriefList focus model', () => {
  listFocusCases((options) => mountBriefList(options))
})
