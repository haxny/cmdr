/** A `SyncRunPanel` mounted over a run that is under way (one step of four done), for its tests. */
import { vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncRunPanel from './SyncRunPanel.svelte'
import { createSyncRun } from './sync-run.svelte'
import { fakeSyncIpc, preview, settle } from './sync-test-fixtures'
import { DEFAULT_SHOW_FILTERS } from './sync-options'

export async function running() {
  const fake = fakeSyncIpc()
  vi.mocked(fake.ipc.getSyncConfirm).mockResolvedValue(
    preview({ groups: [{ key: 'copyRight', files: 2, bytes: 20, keep: null }] }),
  )
  const session = { sessionId: 's1', filters: { ...DEFAULT_SHOW_FILTERS }, compare: vi.fn(), refresh: vi.fn() }
  let now = 1000
  const run = createSyncRun({
    ipc: fake.ipc,
    session,
    now: () => now,
    readDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
  })
  await run.open()
  await run.openConfirm()
  await run.accept()
  now = 5000
  fake.runProgress({ stepsDone: 1, stepsTotal: 4, itemsDone: 1, itemsTotal: 2, bytesDone: 10, bytesTotal: 20 })
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SyncRunPanel, { target, props: { run, onShowFolder: vi.fn(), onShowInGrid: vi.fn() } })
  await settle()
  await tick()
  return { fake, target }
}
