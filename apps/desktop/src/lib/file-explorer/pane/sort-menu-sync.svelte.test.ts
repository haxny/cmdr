/**
 * Tests for `sort-menu-sync.svelte.ts`: View > Sort by's checks follow the focused
 * pane's REQUESTED sort through one deduped push.
 *
 * Seeds the `explorerState` singleton through its named mutators, and spies on the
 * one IPC wrapper. The snapshot store is mocked down to the three reads the sync
 * makes, with a `$state` tick standing in for the real one.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { flushSync } from 'svelte'
import type { SortColumn, SortOrder } from '../types'

const { updateSortMenuSpy, snapshots, tick } = vi.hoisted(() => ({
  updateSortMenuSpy: vi.fn<(sort: unknown) => Promise<void>>(),
  snapshots: new Map<string, { sort: { column: SortColumn; order: SortOrder; effectiveSortBy: SortColumn } | null }>(),
  tick: { bump: (): void => {}, read: (): number => 0 },
}))

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/tauri-commands/index.js')>()),
  updateSortMenu: updateSortMenuSpy,
}))
vi.mock('#lib/search/snapshot-store.svelte.js', () => ({
  getMutationTick: () => tick.read(),
  getSnapshot: (id: string) => snapshots.get(id),
  snapshotIdFromPanePath: (path: string) =>
    path.startsWith('search-results://') ? path.slice('search-results://'.length) : null,
}))

import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { addTab, createTabManager, getActiveTab, switchTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'
import { startSortMenuSync } from './sort-menu-sync.svelte'

let tickValue = $state(0)
tick.read = () => tickValue
tick.bump = () => {
  tickValue += 1
}

function mgrSortedBy(sortBy: SortColumn, sortOrder: SortOrder, path = '/dir'): TabManager {
  const tab = createInitialTabState(path, 'root')
  tab.sortBy = sortBy
  tab.sortOrder = sortOrder
  return createTabManager(tab)
}

function pushes(): unknown[] {
  return updateSortMenuSpy.mock.calls.map(([sort]) => sort)
}

describe('sort-menu-sync', () => {
  let stop: () => void

  beforeEach(() => {
    _resetForTesting()
    updateSortMenuSpy.mockReset()
    updateSortMenuSpy.mockResolvedValue(undefined)
    snapshots.clear()
    explorerState.setTabMgr('left', mgrSortedBy('name', 'ascending'))
    explorerState.setTabMgr('right', mgrSortedBy('size', 'descending'))
    stop = startSortMenuSync()
    flushSync()
  })

  afterEach(() => {
    stop()
  })

  it('pushes the focused pane’s sort on start', () => {
    expect(pushes()).toEqual([{ column: 'name', order: 'ascending' }])
  })

  it('pushes again when focus moves to the other pane', () => {
    explorerState.setFocusedPane('right')
    flushSync()
    expect(pushes().at(-1)).toEqual({ column: 'size', order: 'descending' })
  })

  it('pushes when the focused tab’s sort changes, and nothing for the other pane’s', () => {
    getActiveTab(explorerState.getTabMgr('right')).sortBy = 'extension'
    flushSync()
    expect(pushes()).toHaveLength(1)

    const tab = getActiveTab(explorerState.getTabMgr('left'))
    tab.sortBy = 'added'
    tab.sortOrder = 'descending'
    flushSync()
    expect(pushes().at(-1)).toEqual({ column: 'added', order: 'descending' })
  })

  it('pushes the new tab’s sort on a tab switch', () => {
    const mgr = explorerState.getTabMgr('left')
    const second = createInitialTabState('/other', 'root')
    second.sortBy = 'modified'
    second.sortOrder = 'descending'
    addTab(mgr, getActiveTab(mgr).id, second)
    switchTab(mgr, second.id, null)
    flushSync()
    expect(pushes().at(-1)).toEqual({ column: 'modified', order: 'descending' })
  })

  it('pushes the swapped-in tab manager’s sort on a pane swap', () => {
    const left = explorerState.getTabMgr('left')
    const right = explorerState.getTabMgr('right')
    explorerState.setTabMgr('left', right)
    explorerState.setTabMgr('right', left)
    flushSync()
    expect(pushes().at(-1)).toEqual({ column: 'size', order: 'descending' })
  })

  it('pushes once per distinct value', () => {
    // Same sort on the other pane: focus moves, the value doesn't.
    explorerState.setTabMgr('right', mgrSortedBy('name', 'ascending'))
    explorerState.setFocusedPane('right')
    flushSync()
    getActiveTab(explorerState.getTabMgr('right')).path = '/elsewhere'
    flushSync()
    expect(pushes()).toHaveLength(1)
  })

  it('pushes a search pane’s snapshot sort, and nothing checked in its ranked order', () => {
    snapshots.set('sr-7', { sort: null })
    explorerState.setTabMgr('right', mgrSortedBy('size', 'descending', 'search-results://sr-7'))
    explorerState.setFocusedPane('right')
    flushSync()
    expect(pushes().at(-1)).toBeNull()

    snapshots.set('sr-7', { sort: { column: 'added', order: 'ascending', effectiveSortBy: 'modified' } })
    tick.bump()
    flushSync()
    // ❗ The requested column: the snapshot's fallback to Modified never moves the check.
    expect(pushes().at(-1)).toEqual({ column: 'added', order: 'ascending' })
  })
})
