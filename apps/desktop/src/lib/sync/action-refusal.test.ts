/** The words for rows a key left alone; the backend decides which and why (`set_sync_actions`). */
import { describe, expect, it } from 'vitest'
import { actionsNotice, overrideRefusalText, skippedText } from './action-refusal'

describe('skippedText', () => {
  it('counts the rows and says why, per reason', () => {
    expect(skippedText({ reason: { type: 'sideReadOnly', side: 'right' }, count: 3 })).toBe(
      '3 rows weren’t changed: the right side takes no changes.',
    )
    expect(skippedText({ reason: { type: 'illegalForMode', action: 'removeRight' }, count: 1 })).toBe(
      '1 row wasn’t changed: Remove on the right isn’t possible in this mode.',
    )
    expect(skippedText({ reason: { type: 'illegalForCategory', action: 'copyRight' }, count: 2 })).toBe(
      '2 rows weren’t changed: Copy to the right isn’t possible for those rows.',
    )
    expect(skippedText({ reason: { type: 'nothingToCopy', side: 'left' }, count: 1_200 })).toBe(
      '1,200 rows weren’t changed: there’s nothing on the left to copy.',
    )
    expect(skippedText({ reason: { type: 'cantReverse' }, count: 1 })).toContain('can be reversed')
    expect(skippedText({ reason: { type: 'nothingToToggle' }, count: 1 })).toContain('no action to skip or restore')
  })
})

describe('actionsNotice', () => {
  it('joins every reason and the hidden rows, and says nothing when every row changed', () => {
    expect(actionsNotice([{ reason: { type: 'sideReadOnly', side: 'left' }, count: 2 }], 1)).toBe(
      '2 rows weren’t changed: the left side takes no changes. 1 hidden row wasn’t changed.',
    )
    expect(actionsNotice([], 0)).toBeNull()
  })
})

describe('overrideRefusalText', () => {
  it('words a typed refusal of one row', () => {
    expect(overrideRefusalText({ type: 'noSuchItem', item: 3 })).toBe(
      'That row isn’t in the list anymore. Compare again.',
    )
    expect(overrideRefusalText({ type: 'sideReadOnly', item: 1, side: 'right' })).toBe(
      'The right side takes no changes.',
    )
  })
})
