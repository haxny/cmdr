/** Tier 3 a11y tests for `SyncRunPanel.svelte`: while running (bar, ETA, Pause, Cancel) and finished (the result). */
import { describe, it } from 'vitest'
import { tick } from 'svelte'
import { report, settle } from './sync-test-fixtures'
import { running } from './sync-run-panel-fixture'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

describe('SyncRunPanel a11y', () => {
  it('while running has no violations', async () => {
    const { target } = await running()
    await expectNoA11yViolations(target)
  })

  it('while stopping has no violations', async () => {
    const { target } = await running()
    target.querySelector<HTMLButtonElement>('.actions button')?.click()
    await settle()
    await tick()
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
        heldByGuard: [{ item: 2, block: { type: 'emptyRoot', side: 'right' } }],
        emptyFolders: [{ side: 'right', path: '/b/new' }],
        leftoverFolders: [{ place: 'target', side: 'right', path: '/b/.cmdr-sync/versions/2026-10-31' }],
      }),
    )
    await settle()
    await tick()
    await expectNoA11yViolations(target)
  })
})
