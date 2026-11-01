/**
 * Tier 3 a11y tests for the Synchronize folders grid and its neighbours: the grid with root rows, a folder header,
 * a one-sided folder, a warning row with a pending content compare, a selected row, and an undecided row; the
 * show-filter buttons with some hiding rows; and the summary bar with clickable counts and a guard.
 */
import { describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SyncGrid from './SyncGrid.svelte'
import SyncFilterBar from './SyncFilterBar.svelte'
import SyncSummaryBar from './SyncSummaryBar.svelte'
import { createSyncSession } from './sync-session.svelte'
import { DEFAULT_SHOW_FILTERS } from './sync-options'
import { ROOTS, STATE, fakeSyncIpc, settle } from './sync-test-fixtures'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

async function comparedSession() {
  const fake = fakeSyncIpc()
  const session = createSyncSession({
    ipc: fake.ipc,
    roots: ROOTS,
    readDefaults: () => ({
      keep: { place: 'trashElseVersions', location: { targetRoot: { folderName: '.cmdr-sync/versions' } } },
      timeToleranceSecs: 2,
      ignoreHourShift: false,
      guards: { maxRemovePercent: 50, maxRemoveCount: 0, sentinel: null },
    }),
    includeHidden: () => false,
  })
  await session.open()
  fake.finish()
  await settle()
  return session
}

describe('SyncGrid a11y', () => {
  it('a grid with folder header rows, a warning row, and a selected row has no violations', async () => {
    const session = await comparedSession()
    session.selected.add(3)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SyncGrid, {
      target,
      props: {
        session,
        cursorIndex: 4,
        onCursorChange: vi.fn(),
        onKeydown: vi.fn(),
        onRowClick: vi.fn(),
        onRowDoubleClick: vi.fn(),
        onRowContextMenu: vi.fn(),
      },
    })
    await settle()
    await tick()
    const rows = target.querySelectorAll('[role="row"][aria-label]')
    expect(rows.length).toBe(5)
    expect(target.querySelector('[role="rowheader"]')?.textContent).toContain('Photos/2024')
    // A folder header is named by its own text: no empty `aria-label` hides it.
    expect(target.querySelector('[role="rowheader"]')?.closest('[role="row"]')?.hasAttribute('aria-label')).toBe(false)
    expect(target.querySelector('[data-index="4"]')?.getAttribute('aria-label')).toContain('older replaces newer')
    await expectNoA11yViolations(target)
  })
})

describe('SyncFilterBar a11y', () => {
  it('toggle buttons, some hiding rows, have no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SyncFilterBar, {
      target,
      props: { filters: { ...DEFAULT_SHOW_FILTERS, warningsOnly: true }, onToggle: vi.fn() },
    })
    await tick()
    expect(target.querySelectorAll('[aria-pressed]')).toHaveLength(8)
    await expectNoA11yViolations(target)
  })
})

describe('SyncSummaryBar a11y', () => {
  it('counts, a guard, and a clock offset have no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const onShow = vi.fn()
    mount(SyncSummaryBar, {
      target,
      props: { state: { ...STATE, guards: { ...STATE.guards, skew: { offsetSecs: 3600, sharePercent: 95 } } }, onShow },
    })
    await tick()
    expect(target.textContent).toContain('Mirror would remove 61% of the right side: held until you confirm.')
    expect(target.querySelector('[aria-live="assertive"]')?.textContent).toContain('61%')
    const equal = [...target.querySelectorAll('button')].find((b) => b.textContent.includes('12 equal'))
    equal?.click()
    expect(onShow).toHaveBeenCalledWith({ equal: true, warningsOnly: false })
    await expectNoA11yViolations(target)
  })
})
