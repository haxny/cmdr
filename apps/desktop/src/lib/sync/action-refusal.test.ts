/**
 * A key on the cursor row that the row can't take is said, never silently dropped: the backend's `bulk_changes`
 * leaves such a row alone, so the dialog names why from the row's own facts first.
 */
import { describe, expect, it } from 'vitest'
import { rowRefusal } from './action-refusal'
import type { SyncItemRow } from './row-label'

function row(patch: Partial<SyncItemRow> = {}): SyncItemRow {
  return {
    type: 'item',
    index: 4,
    leftName: 'a.txt',
    rightName: 'a.txt',
    leftKind: 'file',
    rightKind: 'file',
    leftSize: 1,
    rightSize: 2,
    leftMtime: 2,
    rightMtime: 1,
    category: 'newerLeft',
    rung: 'sizeAndTime',
    content: 'notAsked',
    sameSize: false,
    action: 'copyRight',
    defaultAction: 'copyRight',
    olderOverwritesNewer: false,
    legal: ['copyRight', 'skip'],
    ...patch,
  }
}

describe('rowRefusal', () => {
  it('lets a legal action through', () => {
    expect(rowRefusal(row(), 'skip', 'update')).toBeNull()
    expect(rowRefusal(row(), 'defaultDirection', 'update')).toBeNull()
  })

  it('says the mode forbids a write to the left in Update and a removal outside Mirror', () => {
    expect(rowRefusal(row(), 'copyLeft', 'update')).toEqual({ type: 'illegalForMode', action: 'copyLeft' })
    expect(rowRefusal(row(), 'removeTarget', 'bothWays')).toEqual({ type: 'illegalForMode', action: 'removeRight' })
  })

  it('says which side has nothing to copy', () => {
    const leftOnly = row({ rightName: null, rightKind: null, category: 'leftOnly', legal: ['copyRight', 'skip'] })
    expect(rowRefusal(leftOnly, 'copyLeft', 'bothWays')).toEqual({ type: 'nothingToCopy', side: 'right' })
  })

  it('says a clash row takes skip only', () => {
    const clash = row({ category: 'nameClash', action: 'skip', defaultAction: 'skip', legal: ['skip'] })
    expect(rowRefusal(clash, 'copyRight', 'bothWays')).toEqual({ type: 'illegalForCategory', action: 'copyRight' })
  })

  it('says when reverse or toggle have nothing to work on', () => {
    const one = row({ rightName: null, rightKind: null, category: 'leftOnly' })
    expect(rowRefusal(one, 'reverse', 'bothWays')).toEqual({ type: 'cantReverse' })
    const equal = row({ category: 'equal', action: 'equal', defaultAction: 'equal', legal: ['equal', 'skip'] })
    expect(rowRefusal(equal, 'toggleSkip', 'bothWays')).toEqual({ type: 'nothingToToggle' })
  })

  it('falls back to a plain "not allowed" when the facts don’t say why (a read-only side)', () => {
    expect(
      rowRefusal(row({ action: 'skip', defaultAction: 'skip', legal: ['skip'] }), 'copyRight', 'bothWays'),
    ).toEqual({
      type: 'notAllowed',
      action: 'copyRight',
    })
  })
})
