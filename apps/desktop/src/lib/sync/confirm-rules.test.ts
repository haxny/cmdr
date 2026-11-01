import { describe, expect, it } from 'vitest'
import { acknowledgementsNeeded, confirmBlock, enterMayConfirm, groupHold } from './confirm-rules'
import { preview } from './sync-test-fixtures'

const REMOVALS_OVER_LIMIT = preview({
  groups: [
    { key: 'copyRight', files: 2, bytes: 10, keep: null },
    { key: 'removeRight', files: 61, bytes: 10, keep: { type: 'trash' } },
  ],
  blocks: [{ type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 }],
})

describe('confirm rules', () => {
  it('Enter may confirm a copy-only run', () => {
    const copies = preview({ groups: [{ key: 'copyRight', files: 3, bytes: 30, keep: null }] })
    expect(enterMayConfirm(copies, ['copyRight'])).toBe(true)
  })

  it('Enter may not confirm while older-replaces-newer rows are accepted', () => {
    const older = preview({
      groups: [{ key: 'replaceRight', files: 1, bytes: 10, keep: { type: 'trash' } }],
      olderOverwritesNewer: [{ item: 4, relPath: 'a.txt', leftMtime: 1, rightMtime: 2 }],
      olderOverwritesNewerTotal: 1,
    })
    expect(enterMayConfirm(older, ['replaceRight'])).toBe(false)
    expect(enterMayConfirm(older, [])).toBe(false)
  })

  it('Enter may not confirm any removal', () => {
    const removal = preview({
      groups: [
        { key: 'copyRight', files: 1, bytes: 10, keep: null },
        { key: 'removeRight', files: 1, bytes: 10, keep: { type: 'trash' } },
      ],
    })
    expect(enterMayConfirm(removal, ['copyRight', 'removeRight'])).toBe(false)
  })

  it('unticking the removal group lets Enter confirm again', () => {
    const removal = preview({
      groups: [
        { key: 'copyRight', files: 1, bytes: 10, keep: null },
        { key: 'removeRight', files: 1, bytes: 10, keep: { type: 'trash' } },
      ],
    })
    expect(enterMayConfirm(removal, ['copyRight'])).toBe(true)
  })

  it('a guard that needs a second yes keeps Enter off', () => {
    expect(acknowledgementsNeeded(REMOVALS_OVER_LIMIT)).toEqual(['removeAnyway'])
    const empty = preview({
      groups: [{ key: 'copyRight', files: 9, bytes: 90, keep: null }],
      blocks: [{ type: 'emptyRoot', side: 'right' }],
    })
    expect(acknowledgementsNeeded(empty)).toEqual(['copyIntoEmpty'])
    expect(enterMayConfirm(empty, ['copyRight'])).toBe(false)
  })

  it('only the groups left ticked need their second yes', () => {
    expect(acknowledgementsNeeded(REMOVALS_OVER_LIMIT, ['copyRight'])).toEqual([])
    expect(confirmBlock(REMOVALS_OVER_LIMIT, ['copyRight'], [])).toBeNull()
  })

  it('a ticked group waiting for its second yes blocks the run and says how many files wait', () => {
    expect(confirmBlock(REMOVALS_OVER_LIMIT, ['copyRight', 'removeRight'], [])).toEqual({
      type: 'waiting',
      waits: [{ ack: 'removeAnyway', files: 61 }],
    })
    expect(confirmBlock(REMOVALS_OVER_LIMIT, ['copyRight', 'removeRight'], ['removeAnyway'])).toBeNull()
  })

  it('removals beside an empty root and anything under a missing sentinel are held for good', () => {
    const empty = preview({
      groups: [
        { key: 'copyLeft', files: 9, bytes: 90, keep: null },
        { key: 'removeLeft', files: 2, bytes: 9, keep: { type: 'trash' } },
      ],
      blocks: [{ type: 'emptyRoot', side: 'right' }],
    })
    expect(acknowledgementsNeeded(empty)).toEqual([])
    expect(groupHold(empty, 'copyLeft', [])).toBeNull()
    expect(groupHold(empty, 'removeLeft', [])).toEqual({ kind: 'hard', block: { type: 'emptyRoot', side: 'right' } })
    expect(confirmBlock(empty, [], [])).toEqual({ type: 'nothingTicked' })

    const sentinel = preview({
      groups: [{ key: 'copyLeft', files: 9, bytes: 90, keep: null }],
      blocks: [{ type: 'sentinelMissing', side: 'left' }],
    })
    expect(groupHold(sentinel, 'copyLeft', [])).toEqual({
      kind: 'hard',
      block: { type: 'sentinelMissing', side: 'left' },
    })
    expect(confirmBlock(sentinel, [], [])).toEqual({ type: 'allHeld' })
    expect(enterMayConfirm(sentinel, ['copyLeft'])).toBe(false)
  })

  it('nothing ticked runs nothing, so Enter does nothing', () => {
    const copies = preview({ groups: [{ key: 'copyRight', files: 3, bytes: 30, keep: null }] })
    expect(enterMayConfirm(copies, [])).toBe(false)
    expect(confirmBlock(copies, [], [])).toEqual({ type: 'nothingTicked' })
  })
})
