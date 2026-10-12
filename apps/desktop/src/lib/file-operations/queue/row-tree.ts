/**
 * Which queue rows show the live transfer tree inline, and when they get a
 * collapse toggle. `QueueRow.svelte` renders the tree; the page decides the
 * toggle, since it's a question about the whole list. DETAILS § "The inline tree".
 */

import { treeOperationFor } from '../transfer-tree/tree-operation'
import type { TransferTreeOperation } from '../transfer-tree/row-format'
import type { OperationRow } from './operations-store.svelte'

/**
 * The tree a row shows under its readout, or `null`. Only a running or paused
 * row has work to show: a queued one has written nothing yet, and a failed one
 * has nothing left to watch. Past that, it's the progress dialog's rule
 * (`treeOperationFor`): a copy, move, or compress past its scan, never a reversal.
 */
export function rowTreeOperation(row: OperationRow): TransferTreeOperation | null {
  const { status, operationType, reverses } = row.snapshot
  if (status !== 'running' && status !== 'paused') return null
  const phase = row.progress?.phase ?? null
  return treeOperationFor({ operationType, phase, reversing: phase === 'rolling_back' || reverses !== null })
}

/**
 * Whether the rows with a tree get a collapse toggle: only once two or more
 * show one (the queue runs one operation per lane, so copies on disjoint drives
 * run side by side). A lone tree is always open, so there's nothing to toggle.
 */
export function treesCollapsible(rows: readonly OperationRow[]): boolean {
  let trees = 0
  for (const row of rows) {
    if (rowTreeOperation(row) !== null && ++trees >= 2) return true
  }
  return false
}
