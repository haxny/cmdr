/**
 * Tier 3 a11y tests for `SyncFoldersDialog.svelte` and its options strip: while comparing (live counts, Pause and
 * Stop), and compared (grid, summary, a stale banner after an option change).
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncFoldersDialog from './SyncFoldersDialog.svelte'
import SyncOptionsStrip from './SyncOptionsStrip.svelte'
import * as syncIpcModule from './sync-ipc'
import { createSyncSession } from './sync-session.svelte'
import { ROOTS, fakeSyncIpc, settle, type FakeSyncIpc } from './sync-test-fixtures'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('./sync-ipc', async () => {
  const { fakeSyncIpc: make } = await import('./sync-test-fixtures')
  const fake = make()
  return {
    fake,
    syncIpc: fake.ipc,
    readSyncDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
  }
})

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const fake = (syncIpcModule as unknown as { fake: FakeSyncIpc }).fake

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SyncFoldersDialog, { target, props: { roots: ROOTS, onClose: vi.fn() } })
  await settle()
  await tick()
  return target
}

describe('SyncFoldersDialog a11y', () => {
  it('while comparing has no violations', async () => {
    await mountDialog()
    fake.progress({
      left: { side: 'left', folders: 3, files: 1234, bytes: 10 },
      right: { side: 'right', folders: 2, files: 1200, bytes: 9 },
      content: null,
    })
    await tick()
    await expectNoA11yViolations(document.body)
    document.body.innerHTML = ''
  })

  it('compared, with the grid, the summary, and a guard, has no violations', async () => {
    await mountDialog()
    fake.finish()
    await settle()
    await tick()
    await expectNoA11yViolations(document.body)
    document.body.innerHTML = ''
  })
})

describe('SyncOptionsStrip a11y', () => {
  it('the options with a stale compare have no violations', async () => {
    const other = fakeSyncIpc()
    const session = createSyncSession({
      ipc: other.ipc,
      roots: ROOTS,
      readDefaults: () => ({
        keep: { place: 'trashOnly', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
        timeToleranceSecs: 2,
        ignoreHourShift: false,
        guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
      }),
      includeHidden: () => false,
    })
    await session.open()
    session.setOption('masks', '*.pdf | *.tmp')
    session.setOption('mode', 'mirror')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SyncOptionsStrip, { target, props: { session, onSwap: vi.fn() } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
