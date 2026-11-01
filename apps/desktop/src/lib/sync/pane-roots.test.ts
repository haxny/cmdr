import { describe, expect, it } from 'vitest'
import type { FilePaneAPI } from '#lib/file-explorer/pane/types.js'
import { syncRootsOf } from './pane-roots'

interface FakePane {
  listingId: string
  volumeId: string
  path: string
  selected: number[]
  hasParent: boolean
  sequence: number
}

function pane(p: Partial<FakePane> = {}): FilePaneAPI {
  const f: FakePane = { listingId: 'L', volumeId: 'root', path: '/a', selected: [], hasParent: true, sequence: 7, ...p }
  return {
    getListingId: () => f.listingId,
    getVolumeId: () => f.volumeId,
    getCurrentPath: () => f.path,
    getSelectedIndices: () => f.selected,
    hasParentEntry: () => f.hasParent,
    getLastSequence: () => f.sequence,
  } as unknown as FilePaneAPI
}

function depsOf(left: FilePaneAPI | undefined, right: FilePaneAPI | undefined) {
  return { getPaneRef: (side: 'left' | 'right') => (side === 'left' ? left : right) }
}

describe('syncRootsOf', () => {
  it('takes each pane volume and folder, and selections only when asked', () => {
    const left = pane({ listingId: 'L1', volumeId: 'root', path: '/Users/me/Photos', selected: [3, 1] })
    const right = pane({ listingId: 'R1', volumeId: 'smb-nas', path: '/backup/Photos', hasParent: false, sequence: 2 })
    expect(syncRootsOf(depsOf(left, right), false)).toEqual({
      left: { root: { volumeId: 'root', path: '/Users/me/Photos' }, selection: null },
      right: { root: { volumeId: 'smb-nas', path: '/backup/Photos' }, selection: null },
    })
    expect(syncRootsOf(depsOf(left, right), true)).toEqual({
      left: {
        root: { volumeId: 'root', path: '/Users/me/Photos' },
        // Sorted, and without the `..` row's offset: the backend's own row numbers.
        selection: { listingId: 'L1', indices: [0, 2], expectedSequence: 7 },
      },
      // Nothing selected there: the whole folder.
      right: { root: { volumeId: 'smb-nas', path: '/backup/Photos' }, selection: null },
    })
  })

  it('drops the `..` row from a selection', () => {
    const left = pane({ selected: [0, 2] })
    expect(syncRootsOf(depsOf(left, pane()), true)?.left.selection?.indices).toEqual([1])
  })

  it('answers null when a pane shows no folder listing', () => {
    expect(syncRootsOf(depsOf(pane(), undefined), false)).toBeNull()
    expect(syncRootsOf(depsOf(pane({ listingId: '' }), pane()), false)).toBeNull()
  })
})
