import { describe, expect, it } from 'vitest'
import { acknowledgementsNeeded, enterMayConfirm, hardBlocks } from './confirm-rules'
import { preview } from './sync-test-fixtures'

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
    const guarded = preview({
      groups: [{ key: 'removeRight', files: 61, bytes: 10, keep: { type: 'trash' } }],
      blocks: [{ type: 'maxRemove', side: 'right', removals: 61, targetFiles: 100 }],
    })
    expect(acknowledgementsNeeded(guarded)).toEqual(['removeAnyway'])
    const empty = preview({
      groups: [{ key: 'copyRight', files: 9, bytes: 90, keep: null }],
      blocks: [{ type: 'emptyRoot', side: 'right' }],
    })
    expect(acknowledgementsNeeded(empty)).toEqual(['copyIntoEmpty'])
    expect(enterMayConfirm(empty, ['copyRight'])).toBe(false)
  })

  it('an empty root asks nothing of copies the other way, and a missing sentinel is a hard block', () => {
    const empty = preview({
      groups: [{ key: 'copyLeft', files: 9, bytes: 90, keep: null }],
      blocks: [{ type: 'emptyRoot', side: 'right' }],
    })
    expect(acknowledgementsNeeded(empty)).toEqual([])
    expect(hardBlocks(empty)).toEqual([{ type: 'emptyRoot', side: 'right' }])
    const sentinel = preview({
      groups: [{ key: 'copyLeft', files: 9, bytes: 90, keep: null }],
      blocks: [{ type: 'sentinelMissing', side: 'left' }],
    })
    expect(hardBlocks(sentinel)).toEqual([{ type: 'sentinelMissing', side: 'left' }])
    expect(enterMayConfirm(sentinel, ['copyLeft'])).toBe(false)
  })

  it('nothing ticked runs nothing, so Enter does nothing', () => {
    const copies = preview({ groups: [{ key: 'copyRight', files: 3, bytes: 30, keep: null }] })
    expect(enterMayConfirm(copies, [])).toBe(false)
  })
})
