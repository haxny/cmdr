import { describe, expect, it } from 'vitest'
import type { ViewRowWire } from '#lib/tauri-commands/index.js'
import { joinPath, relDirOf, sideFileOf } from './sync-row-files'
import type { SyncItemRow } from './row-label'

function item(index: number, patch: Partial<SyncItemRow> = {}): SyncItemRow {
  return {
    type: 'item',
    index,
    leftName: 'a.txt',
    rightName: 'a.txt',
    leftKind: 'file',
    rightKind: 'file',
    leftSize: 1,
    rightSize: 1,
    leftMtime: 1,
    rightMtime: 1,
    category: 'equal',
    rung: 'sizeAndTime',
    content: 'notAsked',
    sameSize: true,
    action: 'equal',
    defaultAction: 'equal',
    olderOverwritesNewer: false,
    legal: ['equal', 'skip'],
    ...patch,
  }
}

const VIEW: ViewRowWire[] = [
  item(0),
  { type: 'folder', relDir: 'Photos/2024', ordinal: 1, of: 1 },
  item(1, { leftName: 'x.jpg', rightName: null, rightKind: null }),
  item(2, { leftKind: 'folder', leftName: 'Raw', rightKind: 'folder', rightName: 'Raw' }),
]
const rowAt = (index: number) => Promise.resolve(VIEW[index])
const ROOTS = {
  left: { root: { volumeId: 'root', path: '/Users/me' }, selection: null },
  right: { root: { volumeId: 'smb-nas', path: '/' }, selection: null },
}

describe('sync row files', () => {
  it('joins a root, a folder, and a name, the volume root included', () => {
    expect(joinPath('/Users/me/', 'Photos', 'a.jpg')).toBe('/Users/me/Photos/a.jpg')
    expect(joinPath('/', '', 'a.jpg')).toBe('/a.jpg')
  })

  it('finds the folder from the header above, and none for the root rows', async () => {
    expect(await relDirOf(rowAt, 0)).toBe('')
    expect(await relDirOf(rowAt, 2)).toBe('Photos/2024')
  })

  it('answers each side’s file, or why there is none', async () => {
    expect(await sideFileOf(rowAt, ROOTS, { viewIndex: 2, row: VIEW[2] as SyncItemRow, side: 'left' })).toEqual({
      volumeId: 'root',
      path: '/Users/me/Photos/2024/x.jpg',
    })
    expect(await sideFileOf(rowAt, ROOTS, { viewIndex: 0, row: VIEW[0] as SyncItemRow, side: 'right' })).toEqual({
      volumeId: 'smb-nas',
      path: '/a.txt',
    })
    expect(await sideFileOf(rowAt, ROOTS, { viewIndex: 2, row: VIEW[2] as SyncItemRow, side: 'right' })).toEqual({
      type: 'noFile',
    })
    expect(await sideFileOf(rowAt, ROOTS, { viewIndex: 3, row: VIEW[3] as SyncItemRow, side: 'left' })).toEqual({
      type: 'notAFile',
    })
  })
})
