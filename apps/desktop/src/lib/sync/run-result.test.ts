import { describe, expect, it } from 'vitest'
import { notDoneItems, resultLines } from './run-result'
import { report } from './sync-test-fixtures'

describe('resultLines', () => {
  it('names every way a row ended, by its typed reason, and splits guard holds from failed-copy holds', () => {
    const lines = resultLines(
      report({
        done: 3,
        held: 3,
        items: [
          { item: 0, result: 'done' },
          { item: 1, result: 'changedSinceCompare' },
          { item: 2, result: 'versionsSpotTaken' },
          { item: 3, result: 'couldNotDo' },
          { item: 4, result: 'heldAfterFailedCopy' },
          { item: 5, result: 'keptNotReplaced' },
          { item: 6, result: 'notStarted' },
          { item: 7, result: 'noTrash' },
          { item: 8, result: 'versionsFolderUnsafe' },
        ],
        heldByGuard: [
          { item: 9, block: { type: 'maxRemove', side: 'right', removals: 9, targetFiles: 10 } },
          { item: 10, block: { type: 'emptyRoot', side: 'right' } },
        ],
      }),
    )
    expect(lines).toEqual([
      { kind: 'done', count: 3 },
      { kind: 'changedSinceCompare', count: 1 },
      { kind: 'noTrash', count: 1 },
      { kind: 'couldNotDo', count: 1 },
      { kind: 'versionsSpotTaken', count: 1 },
      { kind: 'versionsFolderUnsafe', count: 1 },
      { kind: 'keptNotReplaced', count: 1 },
      { kind: 'heldAfterFailedCopy', count: 1 },
      { kind: 'heldByGuard', count: 2 },
      { kind: 'notStarted', count: 1 },
    ])
  })

  it('a clean run is one line', () => {
    expect(resultLines(report())).toEqual([{ kind: 'done', count: 1 }])
  })

  it('rows a guard held count as not done, for Show in grid', () => {
    const held = report({
      items: [
        { item: 0, result: 'done' },
        { item: 1, result: 'couldNotDo' },
      ],
      heldByGuard: [{ item: 4, block: { type: 'sentinelMissing', side: 'right' } }],
    })
    expect(notDoneItems(held)).toEqual([1, 4])
  })
})
