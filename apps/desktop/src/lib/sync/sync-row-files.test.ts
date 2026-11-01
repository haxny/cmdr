import { describe, expect, it } from 'vitest'
import { joinPath, sideFileOf } from './sync-row-files'
import { itemRow } from './sync-test-fixtures'

const ROOTS = {
  left: { root: { volumeId: 'root', path: '/Users/me' }, selection: null },
  right: { root: { volumeId: 'smb-nas', path: '/' }, selection: null },
}

describe('sync row files', () => {
  it('joins a root and a path, the volume root included', () => {
    expect(joinPath('/Users/me/', 'Photos/a.jpg')).toBe('/Users/me/Photos/a.jpg')
    expect(joinPath('/', 'a.jpg')).toBe('/a.jpg')
  })

  it('opens each side in its own spelling, case and Unicode form included', () => {
    const row = itemRow(0, { leftRelPath: 'Fotky/café.jpg', rightRelPath: 'fotky/café.jpg' })
    expect(sideFileOf(ROOTS, row, 'left')).toEqual({ volumeId: 'root', path: '/Users/me/Fotky/café.jpg' })
    expect(sideFileOf(ROOTS, row, 'right')).toEqual({ volumeId: 'smb-nas', path: '/fotky/café.jpg' })
  })

  it('says why there is nothing to open', () => {
    expect(sideFileOf(ROOTS, itemRow(0, { rightRelPath: null, rightName: null, rightKind: null }), 'right')).toEqual({
      type: 'noFile',
    })
    expect(sideFileOf(ROOTS, itemRow(0, { leftKind: 'folder' }), 'left')).toEqual({ type: 'notAFile' })
  })
})
