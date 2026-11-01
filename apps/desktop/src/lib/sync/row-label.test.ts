/**
 * A grid row's accessible name says, in words, what colour and arrows show: the name, how the sides relate, what the
 * run does, and whether you changed it.
 */
import { describe, expect, it } from 'vitest'
import { rowAccessibleName, type SyncItemRow } from './row-label'

function row(patch: Partial<SyncItemRow> = {}): SyncItemRow {
  return {
    type: 'item',
    index: 0,
    leftName: 'report.pdf',
    rightName: 'report.pdf',
    leftKind: 'file',
    rightKind: 'file',
    leftSize: 10,
    rightSize: 12,
    leftMtime: 2_000,
    rightMtime: 1_000,
    category: 'newerLeft',
    rung: 'sizeAndTime',
    content: 'notAsked',
    sameSize: false,
    action: 'copyRight',
    defaultAction: 'copyRight',
    olderOverwritesNewer: false,
    legal: ['copyRight', 'copyLeft', 'skip'],
    ...patch,
  }
}

describe('rowAccessibleName', () => {
  it('names the file, category, action, and "changed by you" in words', () => {
    expect(rowAccessibleName(row({ defaultAction: 'skip' }))).toBe(
      'report.pdf, newer on the left, will be copied to the right, changed by you',
    )
  })

  it('leaves out "changed by you" when the mode chose the action', () => {
    expect(rowAccessibleName(row())).toBe('report.pdf, newer on the left, will be copied to the right')
  })

  it('says "older replaces newer" for the warning category', () => {
    const name = rowAccessibleName(
      row({ category: 'newerRight', action: 'copyRight', defaultAction: 'copyRight', olderOverwritesNewer: true }),
    )
    expect(name).toBe('report.pdf, newer on the right, will be copied to the right, older replaces newer')
  })

  it('names a folder on one side, a removal, and an undecided row', () => {
    expect(
      rowAccessibleName(
        row({
          leftName: null,
          leftKind: null,
          rightName: 'Old',
          rightKind: 'folder',
          category: 'rightOnly',
          action: 'removeRight',
          defaultAction: 'removeRight',
        }),
      ),
    ).toBe('Old, folder, only on the right, will be removed on the right')
    expect(rowAccessibleName(row({ category: 'unknown', action: 'undecided', defaultAction: 'undecided' }))).toBe(
      'report.pdf, can’t tell which is newer, undecided',
    )
  })

  it('says when the content is being compared', () => {
    expect(rowAccessibleName(row({ content: 'pending' }))).toBe(
      'report.pdf, newer on the left, will be copied to the right, comparing content',
    )
  })

  it('names both spellings when the sides spell the name differently', () => {
    expect(rowAccessibleName(row({ leftName: 'Report.pdf', rightName: 'report.pdf' }))).toMatch(
      /^Report\.pdf \/ report\.pdf, /,
    )
  })
})
