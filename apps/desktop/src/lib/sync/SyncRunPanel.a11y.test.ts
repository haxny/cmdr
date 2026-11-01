/** Tier 3 a11y tests for `SyncRunPanel.svelte`: while running (bar, ETA, Pause, Cancel) and finished (the result). */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncRunPanel from './SyncRunPanel.svelte'
import { createSyncRun } from './sync-run.svelte'
import { fakeSyncIpc, preview, report, settle } from './sync-test-fixtures'
import { DEFAULT_SHOW_FILTERS } from './sync-options'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

async function running() {
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

describe('SyncRunPanel a11y', () => {
  it('while running has no violations', async () => {
    const { target } = await running()
    await expectNoA11yViolations(target)
  })

  it('finished has no violations', async () => {
    const { fake, target } = await running()
    fake.runFinished(
      report({
        done: 1,
        held: 1,
        items: [
          { item: 0, result: 'done' },
          { item: 1, result: 'changedSinceCompare' },
        ],
        versionsFolders: [{ type: 'folder', volumeId: 'root', path: '/b/.cmdr-sync/versions/2026-11-01' }],
        keptBytes: 4096,
      }),
    )
    await settle()
    await tick()
    await expectNoA11yViolations(target)
  })
})
