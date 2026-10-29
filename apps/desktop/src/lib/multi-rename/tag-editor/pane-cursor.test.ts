/** A pane's cursor keys: bare keys move, ⇧ also extends, and a move that hits an end says so (overflow). */
import { describe, expect, it } from 'vitest'
import { editorBox, paneCursorMove } from './pane-cursor'

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

describe('editorBox', () => {
  const rect = (left: number, top: number, width: number, height: number): DOMRect =>
    ({
      left,
      top,
      width,
      height,
      right: left + width,
      bottom: top + height,
      x: left,
      y: top,
      toJSON: () => ({}),
    }) as DOMRect

  function boxes(scrollTop: number, scrollLeft: number, cell: DOMRect) {
    const container = document.createElement('div')
    const el = document.createElement('div')
    container.getBoundingClientRect = () => rect(10, 20, 400, 300)
    el.getBoundingClientRect = () => cell
    container.scrollTop = scrollTop
    container.scrollLeft = scrollLeft
    return editorBox(container, el)
  }

  it('equals the cell box in the scroller coordinates, height included', () => {
    expect(boxes(0, 0, rect(30, 70, 120, 22))).toEqual({ top: 50, left: 20, width: 120, height: 22 })
  })

  it('adds the scroll offsets so the box scrolls with its row', () => {
    const place = boxes(40, 0, rect(30, 70, 120, 22))
    expect(place.top).toBe(90)
  })

  it('keeps a squeezed column wide enough to type in', () => {
    expect(boxes(0, 0, rect(30, 70, 8, 22)).width).toBe(40)
  })
})
