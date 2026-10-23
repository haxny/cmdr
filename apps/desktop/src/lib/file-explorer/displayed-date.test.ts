import { describe, expect, it } from 'vitest'
import type { SortColumn } from './types'
import {
  dateHeaderSortColumn,
  dateOf,
  displayedDateColumn,
  headerLabelKey,
  rowDatePhraseKey,
  TOOLTIP_DATE_ORDER,
  tooltipLabelKey,
} from './displayed-date'

describe('displayedDateColumn', () => {
  it.each<[SortColumn | null, string]>([
    ['created', 'created'],
    ['added', 'added'],
    ['modified', 'modified'],
    ['name', 'modified'],
    ['extension', 'modified'],
    ['size', 'modified'],
    // A search pane in relevance order, or a pane no listing has landed in yet.
    [null, 'modified'],
  ])('shows %s rows the %s date', (effective, shown) => {
    expect(displayedDateColumn(effective)).toBe(shown)
  })

  // The rows follow Modified when a location doesn't record the requested date, and
  // the pane hands over the EFFECTIVE column, so a fallback shows Modified.
  it('shows Modified when an Added sort fell back to Modified', () => {
    const requested: SortColumn = 'added'
    const effective: SortColumn = 'modified'
    expect(displayedDateColumn(effective)).not.toBe(requested)
    expect(displayedDateColumn(effective)).toBe('modified')
  })
})

describe('dateOf', () => {
  const entry = { modifiedAt: 300, createdAt: 100, addedAt: 200 }

  it('reads the date each column names', () => {
    expect(dateOf(entry, 'modified')).toBe(300)
    expect(dateOf(entry, 'created')).toBe(100)
    expect(dateOf(entry, 'added')).toBe(200)
  })

  // IPC sends `None` as `null`, which must read as "no date", never as the epoch.
  it('answers undefined for a date the entry lacks, null included', () => {
    expect(dateOf({ modifiedAt: 1 }, 'added')).toBeUndefined()
    expect(dateOf({ modifiedAt: 1, addedAt: null as unknown as undefined }, 'added')).toBeUndefined()
  })
})

describe('dateHeaderSortColumn', () => {
  it.each<[SortColumn | null, SortColumn]>([
    ['modified', 'modified'],
    ['created', 'created'],
    ['added', 'added'],
    ['name', 'modified'],
    ['size', 'modified'],
    [null, 'modified'],
  ])('sorts by %s through the date header as %s', (requested, column) => {
    expect(dateHeaderSortColumn(requested)).toBe(column)
  })
})

describe('message keys', () => {
  it('names each date in the header, the tooltip, and the row name', () => {
    expect(headerLabelKey('modified')).toBe('fileExplorer.columns.modified')
    expect(headerLabelKey('created')).toBe('fileExplorer.columns.created')
    expect(headerLabelKey('added')).toBe('fileExplorer.columns.added')
    expect(tooltipLabelKey('modified')).toBe('fileExplorer.dateTooltip.lastModified')
    expect(tooltipLabelKey('created')).toBe('fileExplorer.dateTooltip.created')
    expect(tooltipLabelKey('added')).toBe('fileExplorer.dateTooltip.dateAdded')
    expect(rowDatePhraseKey('modified')).toBe('fileExplorer.list.rowModified')
    expect(rowDatePhraseKey('created')).toBe('fileExplorer.list.rowCreated')
    expect(rowDatePhraseKey('added')).toBe('fileExplorer.list.rowAdded')
  })

  it('lists tooltip dates as Created, Date added, Last modified', () => {
    expect(TOOLTIP_DATE_ORDER).toEqual(['created', 'added', 'modified'])
  })
})
