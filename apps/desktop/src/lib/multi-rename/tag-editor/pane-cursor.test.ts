/** A pane's cursor keys: bare keys move, ⇧ also extends, and a move that hits an end says so (overflow). */
import { describe, expect, it } from 'vitest'
import { editorBox, needsLargeEditor, paneCursorMove } from './pane-cursor'

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

  it('takes the ROW height and top when the cell is empty and only its padding tall (the thin-strip bug)', () => {
    const container = document.createElement('div')
    const row = document.createElement('div')
    row.className = 'file-entry'
    const el = document.createElement('span')
    row.append(el)
    container.append(row)
    container.getBoundingClientRect = () => rect(10, 20, 400, 300)
    row.getBoundingClientRect = () => rect(14, 60, 380, 22)
    el.getBoundingClientRect = () => rect(30, 65, 120, 5)
    expect(editorBox(container, el)).toEqual({ top: 40, left: 20, width: 120, height: 22 })
  })

  it('adds the scroll offsets so the box scrolls with its row', () => {
    const place = boxes(40, 0, rect(30, 70, 120, 22))
    expect(place.top).toBe(90)
  })

  it('keeps a squeezed column wide enough to type in', () => {
    expect(boxes(0, 0, rect(30, 70, 8, 22)).width).toBe(40)
  })
})

describe('needsLargeEditor', () => {
  const cell = (scrollWidth: number, clientWidth: number): HTMLElement => {
    const el = document.createElement('span')
    Object.defineProperty(el, 'scrollWidth', { value: scrollWidth })
    Object.defineProperty(el, 'clientWidth', { value: clientWidth })
    return el
  }

  it('is true for a value cut with … and for one holding a line break', () => {
    expect(needsLargeEditor(cell(400, 120), 'long')).toBe(true)
    expect(needsLargeEditor(cell(100, 100), 'a\nb')).toBe(true)
  })

  it('is false for a short value, so it stays on one line', () => {
    expect(needsLargeEditor(cell(100, 100), 'short')).toBe(false)
  })
})
