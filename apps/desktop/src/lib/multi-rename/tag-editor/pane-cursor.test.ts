/** A pane's cursor keys: bare keys move, ⇧ also extends, and a move that hits an end says so (overflow). */
import { describe, expect, it } from 'vitest'
import { paneCursorMove } from './pane-cursor'

const key = (k: string, init: KeyboardEventInit = {}) => new KeyboardEvent('keydown', { key: k, ...init })

describe('paneCursorMove', () => {
  it('↓ steps, End jumps to the last row, other keys are not cursor keys', () => {
    expect(paneCursorMove(key('ArrowDown'), { count: 4, from: 1, visibleRows: 10 })).toEqual({
      to: 2,
      extend: false,
      overflow: false,
    })
    expect(paneCursorMove(key('End'), { count: 4, from: 1, visibleRows: 10 })).toEqual({
      to: 3,
      extend: false,
      overflow: true,
    })
    expect(paneCursorMove(key('a'), { count: 4, from: 1, visibleRows: 10 })).toBeNull()
  })

  it('⇧↓ extends; ↓ on the last row overflows; ⌘↓ is not a cursor key', () => {
    expect(paneCursorMove(key('ArrowDown', { shiftKey: true }), { count: 4, from: 0, visibleRows: 10 })).toEqual({
      to: 1,
      extend: true,
      overflow: false,
    })
    expect(paneCursorMove(key('ArrowDown', { shiftKey: true }), { count: 4, from: 3, visibleRows: 10 })).toEqual({
      to: 3,
      extend: true,
      overflow: true,
    })
    expect(paneCursorMove(key('ArrowDown', { metaKey: true }), { count: 4, from: 0, visibleRows: 10 })).toBeNull()
  })

  it('PageDown moves a screen less one row, and an empty list has no cursor', () => {
    expect(paneCursorMove(key('PageDown'), { count: 30, from: 0, visibleRows: 10 })).toEqual({
      to: 9,
      extend: false,
      overflow: false,
    })
    expect(paneCursorMove(key('ArrowDown'), { count: 0, from: 0, visibleRows: 10 })).toBeNull()
  })
})
