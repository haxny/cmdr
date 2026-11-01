/**
 * Tier 3 a11y tests for `SyncConfirmDialog.svelte`: a Mirror confirm with the older-replaces-newer warning, a removal
 * group past the limit with its "Remove anyway", an empty root, `?` rows chosen, and hidden actions; and a plain
 * copy-only confirm.
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncConfirmDialog from './SyncConfirmDialog.svelte'
import { createSyncRun } from './sync-run.svelte'
import { fakeSyncIpc, preview, settle } from './sync-test-fixtures'
import { DEFAULT_SHOW_FILTERS } from './sync-options'
import type { ConfirmPreview } from '#lib/tauri-commands/sync.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

async function mountConfirm(shown: ConfirmPreview): Promise<HTMLElement> {
  const fake = fakeSyncIpc()
  vi.mocked(fake.ipc.getSyncConfirm).mockResolvedValue(shown)
  const session = { sessionId: 's1', filters: { ...DEFAULT_SHOW_FILTERS }, compare: vi.fn(), refresh: vi.fn() }
  const run = createSyncRun({
    ipc: fake.ipc,
    session,
    readDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
  })
  await run.openConfirm()
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SyncConfirmDialog, { target, props: { run } })
  await settle()
  await tick()
  return target
}

describe('SyncConfirmDialog a11y', () => {
  it('with warnings and guard blocks has no violations', async () => {
    const target = await mountConfirm(
      preview({
        groups: [
          { key: 'copyRight', files: 2, bytes: 2048, keep: null },
          { key: 'replaceRight', files: 1, bytes: 10, keep: { type: 'folder', volumeId: 'nas', path: '/v/versions' } },
          { key: 'removeRight', files: 61, bytes: 6100, keep: { type: 'trash' } },
        ],
        olderOverwritesNewer: [{ item: 3, relPath: 'a/b.txt', leftMtime: 1_700_000_000, rightMtime: null }],
        olderOverwritesNewerTotal: 4,
        blocks: [
          { type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 },
          { type: 'emptyRoot', side: 'left' },
        ],
        skew: { offsetSecs: 3600, sharePercent: 95 },
        hiddenActions: 12,
        chosenUnknown: 2,
        keepIsSlow: true,
      }),
    )
    await expectNoA11yViolations(target.ownerDocument.body)
  })

  it('a copy-only confirm has no violations', async () => {
    const target = await mountConfirm(preview({ groups: [{ key: 'copyLeft', files: 1, bytes: 10, keep: null }] }))
    await expectNoA11yViolations(target.ownerDocument.body)
  })
})
