/**
 * Which date a file list shows, and the words each date goes by.
 *
 * ❗ `displayedDateColumn` is the ONE derivation of the shown date: every surface that
 * shows a date (Full cells, the Full date header's label and measured width, the date
 * tooltips' "(displayed)" marker, Full rows' accessible names) reads it, ❌ never a
 * second rule of its own. It takes the EFFECTIVE column, the one the backend actually
 * ordered the rows by, so a Created or Added sort a location can't honor shows the
 * Modified date its rows follow (`crates/cmdr-listing/DETAILS.md` § "Requested and
 * effective sort"). ❌ No "last date sort" memory: a Name sort shows Modified.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { FileEntry, SortColumn } from './types'

/** A sort column that is a date, and so a date a list can show. */
export type DateColumn = Extract<SortColumn, 'modified' | 'created' | 'added'>

/**
 * The date the rows show: the one they're sorted by when that's Created or Added,
 * else Modified. `null` is a pane in no column's order (a search pane ranked by
 * relevance, or one no listing has landed in yet).
 */
export function displayedDateColumn(effectiveSortBy: SortColumn | null): DateColumn {
  return effectiveSortBy === 'created' || effectiveSortBy === 'added' ? effectiveSortBy : 'modified'
}

/**
 * The column the Full date header sorts by and lights up for: the REQUESTED sort when
 * it's a date, so a click flips that sort's order, else Modified.
 */
export function dateHeaderSortColumn(requested: SortColumn | null): DateColumn {
  return requested === 'modified' || requested === 'created' || requested === 'added' ? requested : 'modified'
}

/** The entry's `column` date in Unix seconds, or `undefined` when it has none. */
export function dateOf(
  entry: Pick<FileEntry, 'modifiedAt' | 'createdAt' | 'addedAt'>,
  column: DateColumn,
): number | undefined {
  const date = column === 'created' ? entry.createdAt : column === 'added' ? entry.addedAt : entry.modifiedAt
  // `?? undefined` because IPC sends `Option::None` as `null`.
  return date ?? undefined
}

/** The order the date tooltips list an entry's dates in (Finder's order). */
export const TOOLTIP_DATE_ORDER: readonly DateColumn[] = ['created', 'added', 'modified']

/** The Full date column's header label. */
export function headerLabelKey(column: DateColumn): MessageKey {
  return HEADER_LABEL_KEYS[column]
}

/** The date's line label in a date tooltip. */
export function tooltipLabelKey(column: DateColumn): MessageKey {
  return TOOLTIP_LABEL_KEYS[column]
}

/** The phrase around the date in a Full row's accessible name ("modified {date}"). */
export function rowDatePhraseKey(column: DateColumn): RowDatePhraseKey {
  return ROW_DATE_PHRASE_KEYS[column]
}

export type RowDatePhraseKey =
  | 'fileExplorer.list.rowModified'
  | 'fileExplorer.list.rowCreated'
  | 'fileExplorer.list.rowAdded'

const HEADER_LABEL_KEYS: Record<DateColumn, MessageKey> = {
  modified: 'fileExplorer.columns.modified',
  created: 'fileExplorer.columns.created',
  added: 'fileExplorer.columns.added',
}

const TOOLTIP_LABEL_KEYS: Record<DateColumn, MessageKey> = {
  modified: 'fileExplorer.dateTooltip.lastModified',
  created: 'fileExplorer.dateTooltip.created',
  added: 'fileExplorer.dateTooltip.dateAdded',
}

const ROW_DATE_PHRASE_KEYS: Record<DateColumn, RowDatePhraseKey> = {
  modified: 'fileExplorer.list.rowModified',
  created: 'fileExplorer.list.rowCreated',
  added: 'fileExplorer.list.rowAdded',
}
