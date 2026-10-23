import { SORT_BY_COMMANDS, type CommandId, type SortColumn } from '#lib/commands/index.js'

const SORT_ORDER_COMMANDS: Record<string, CommandId> = {
  asc: 'sort.ascending',
  desc: 'sort.descending',
  toggle: 'sort.toggleOrder',
}

function isSortColumn(value: string): value is SortColumn {
  return Object.hasOwn(SORT_BY_COMMANDS, value)
}

/**
 * Maps a native `menu-sort` payload onto a focused-pane `sort.*` command id,
 * or `undefined` for an unrecognized payload. `sortBy` selects the column;
 * `sortOrder` selects ascending/descending (the menu never emits `toggle`).
 */
export function menuSortToCommand(action: unknown, value: unknown): CommandId | undefined {
  if (typeof value !== 'string') return undefined
  if (action === 'sortBy') return isSortColumn(value) ? SORT_BY_COMMANDS[value] : undefined
  if (action === 'sortOrder') return Object.hasOwn(SORT_ORDER_COMMANDS, value) ? SORT_ORDER_COMMANDS[value] : undefined
  return undefined
}
