/**
 * Keeps View > Sort by's checkmarks on the sort the focused pane ASKED for.
 *
 * ONE `$effect` derives the focused pane's requested sort and pushes it on change,
 * deduped, so focus, tab switch, pane swap, and every sort path (palette, menu,
 * header, MCP) reach the menu through this one derivation, ❌ never a push per path.
 * A pane swap trades tab managers, and a sort writes its tab or its snapshot, both of
 * which this reads.
 *
 * ❗ The requested column, never a fallback's effective one: a date sort the location
 * can't honor still checks that date (the requested-vs-effective rule,
 * `file-explorer/DETAILS.md` § Sorting).
 *
 * Main window only: it's the only window with panes. A language rebuild of the bar
 * needs no re-push, because Rust seeds the new bar from what it was last told.
 */

import { getMutationTick, getSnapshot, snapshotIdFromPanePath } from '#lib/search/snapshot-store.svelte.js'
import { updateSortMenu, type RequestedSort } from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { getActiveTab } from '../tabs/tab-state-manager.svelte'
import { explorerState } from './explorer-state.svelte'

const log = getAppLogger('fileExplorer')

/**
 * The sort the focused pane asked for, or `null` for a search pane in its ranked
 * order. A search pane's sort lives in its snapshot, ❌ not its tab (whose sort
 * belongs to the folder the user came from). Reactive.
 */
export function focusedPaneRequestedSort(): RequestedSort | null {
  const tab = getActiveTab(explorerState.getTabMgr(explorerState.getFocusedPane()))
  const snapshotId = snapshotIdFromPanePath(tab.path)
  if (snapshotId === null) return { column: tab.sortBy, order: tab.sortOrder }
  // Snapshots aren't `$state`; the tick is what a re-sort bumps.
  getMutationTick()
  const sort = getSnapshot(snapshotId)?.sort ?? null
  return sort === null ? null : { column: sort.column, order: sort.order }
}

let stopSync: (() => void) | null = null

/**
 * Starts pushing the focused pane's requested sort to the native menu, and returns
 * the stop function. Idempotent: a second call keeps the first scope rather than
 * stacking another (HMR re-runs main-window startup).
 */
export function startSortMenuSync(): () => void {
  if (stopSync) return stopSync

  const dispose = $effect.root(() => {
    let lastSent: string | null = null
    $effect(() => {
      const sort = focusedPaneRequestedSort()
      const key = sort === null ? 'none' : `${sort.column}:${sort.order}`
      if (key === lastSent) return
      lastSent = key
      void updateSortMenu(sort).catch((err: unknown) => {
        // The checks stay where they were; sorting itself is unaffected.
        log.warn("Couldn't update View > Sort by's checkmarks: {error}", { error: err })
      })
    })
  })

  stopSync = () => {
    dispose()
    stopSync = null
  }
  return stopSync
}
