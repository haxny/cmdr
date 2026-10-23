import type { CommandId } from './command-ids'
import type { SortColumn } from './types'

/**
 * The focused-pane command that sorts by each column. One table for the column
 * headers (whose tooltips name the command and its shortcut) and the native
 * View > Sort by items, so a new column can't reach one and miss the other: the
 * `Record` makes a missing column a compile error.
 */
export const SORT_BY_COMMANDS: Record<SortColumn, CommandId> = {
  name: 'sort.byName',
  extension: 'sort.byExtension',
  size: 'sort.bySize',
  modified: 'sort.byModified',
  created: 'sort.byCreated',
  added: 'sort.byAdded',
}
