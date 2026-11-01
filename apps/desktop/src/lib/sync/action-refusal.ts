/**
 * Why the row under the cursor can't take a key's action. The backend's `bulk_changes` leaves a row that can't take
 * an action alone without a word (right for a selection of many rows), so a key on ONE row is checked here first,
 * from the row's own facts (`legal`, its sides, the mode), and the dialog says why instead of doing nothing.
 */
import type { Action, BulkAction, Mode, Side } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { actionLabel, type SyncItemRow } from './row-label'

export type RowRefusal =
  | { type: 'illegalForMode'; action: Action }
  | { type: 'illegalForCategory'; action: Action }
  | { type: 'nothingToCopy'; side: Side }
  | { type: 'cantReverse' }
  | { type: 'nothingToToggle' }
  /** `legal` leaves it out and the row's facts don't say why (a side that takes no writes). */
  | { type: 'notAllowed'; action: Action }

const isActing = (action: Action): boolean => action !== 'equal' && action !== 'skip' && action !== 'undecided'

/** The action a key asks of this row, `null` when it has none (mirrors `overrides.rs` `wanted`). */
function wantedAction(row: SyncItemRow, bulk: BulkAction): Action | null | 'cantReverse' {
  switch (bulk) {
    case 'defaultDirection':
      return row.defaultAction
    case 'copyRight':
      return 'copyRight'
    case 'copyLeft':
      return 'copyLeft'
    case 'skip':
      return 'skip'
    case 'removeTarget':
      return 'removeRight'
    case 'toggleSkip':
      if (isActing(row.action)) return 'skip'
      return row.action === 'skip' && isActing(row.defaultAction) ? row.defaultAction : null
    case 'reverse':
      return reversed(row)
  }
}

function reversed(row: SyncItemRow): Action | 'cantReverse' {
  const both = row.leftName !== null && row.rightName !== null
  if (row.action === 'copyRight' && both) return 'copyLeft'
  if (row.action === 'copyLeft' && both) return 'copyRight'
  if (row.action === 'removeRight') return 'copyLeft'
  if (row.action === 'removeLeft') return 'copyRight'
  return 'cantReverse'
}

/** A write to the left in Update and Mirror; a removal outside Mirror (and on the left, anywhere but by hand). */
function modeForbids(action: Action, mode: Mode): boolean {
  const writesLeft = action === 'copyLeft' || action === 'removeLeft'
  if (writesLeft && (mode === 'update' || mode === 'mirror')) return true
  const removes = action === 'removeRight' || action === 'removeLeft'
  return removes && mode !== 'mirror'
}

/** The side the action copies from (or removes), which must hold something. */
function sourceSide(action: Action): Side | null {
  if (action === 'copyRight' || action === 'removeLeft') return 'left'
  if (action === 'copyLeft' || action === 'removeRight') return 'right'
  return null
}

/** Why `bulk` can't change this row, or `null` when it can (or would change nothing, which isn't a refusal). */
export function rowRefusal(row: SyncItemRow, bulk: BulkAction, mode: Mode): RowRefusal | null {
  const action = wantedAction(row, bulk)
  if (action === 'cantReverse') return { type: 'cantReverse' }
  if (action === null) return { type: 'nothingToToggle' }
  if (action === row.action || row.legal.includes(action)) return null
  const side = sourceSide(action)
  const empty = side === 'left' ? row.leftName === null : side === 'right' ? row.rightName === null : false
  if (side !== null && empty) return { type: 'nothingToCopy', side }
  if (modeForbids(action, mode)) return { type: 'illegalForMode', action }
  const comparable = row.category !== 'nameClash' && row.category !== 'typeClash' && row.category !== 'unreadable'
  if (!comparable || action === 'equal' || action === 'undecided') return { type: 'illegalForCategory', action }
  return { type: 'notAllowed', action }
}

/** The refusal in words, for the dialog's notice line. */
export function rowRefusalText(refusal: RowRefusal): string {
  switch (refusal.type) {
    case 'illegalForMode':
      return tString('sync.refusal.illegalForMode', { action: actionLabel(refusal.action) })
    case 'illegalForCategory':
      return tString('sync.refusal.illegalForCategory', { action: actionLabel(refusal.action) })
    case 'nothingToCopy':
      return tString(refusal.side === 'left' ? 'sync.refusal.nothingOnLeft' : 'sync.refusal.nothingOnRight')
    case 'cantReverse':
      return tString('sync.refusal.cantReverse')
    case 'nothingToToggle':
      return tString('sync.refusal.nothingToToggle')
    case 'notAllowed':
      return tString('sync.refusal.notAllowed', { action: actionLabel(refusal.action) })
  }
}
