import { describe, expect, it } from 'vitest'
import { syncProblemText } from './sync-errors'

describe('syncProblemText', () => {
  it('names the side, the conflict, and the mask problem by variant', () => {
    expect(syncProblemText({ type: 'rootUnreachable', side: 'right' })).toBe(
      'The right folder can’t be opened. It may be on a drive that isn’t mounted.',
    )
    expect(syncProblemText({ type: 'rootsConflict', conflict: { type: 'nested', outer: 'left' } })).toBe(
      'The right folder is inside the left one. Open two folders apart from each other.',
    )
    expect(syncProblemText({ type: 'badMask', error: { type: 'secondBar', position: 4 } })).toBe(
      'The masks have more than one |. Put every mask to leave out after one |.',
    )
    expect(syncProblemText({ type: 'keepFolderUnreadable' })).toContain('versions folder')
  })

  it('speaks an override refusal as the row refusal it is', () => {
    expect(
      syncProblemText({ type: 'override', refusal: { type: 'illegalForMode', item: 1, action: 'copyLeft' } }),
    ).toBe('Copy to the left isn’t possible in this mode.')
    expect(syncProblemText({ type: 'override', refusal: { type: 'noSuchItem', item: 7 } })).toBe(
      'That row isn’t in the list anymore. Compare again.',
    )
    expect(syncProblemText({ type: 'override', refusal: { type: 'sideReadOnly', item: 1, side: 'left' } })).toBe(
      'The left side takes no changes.',
    )
  })

  it('has words for something it couldn’t classify', () => {
    expect(syncProblemText({ type: 'unexpected' })).toContain('Something went wrong')
  })
})
