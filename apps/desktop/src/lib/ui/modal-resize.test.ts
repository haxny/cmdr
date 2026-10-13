import { describe, it, expect } from 'vitest'
import { declaredHeight, resizeDirectionsFor, resizeStep, type ResizeStepInput } from './modal-resize'

const NO_LIMITS = { minWidth: 0, maxWidth: Infinity, minHeight: 60, maxHeight: Infinity }

/** A 400×300 panel at rest, dragged by (dx, dy). */
function step(overrides: Partial<ResizeStepInput>) {
  return resizeStep({
    direction: 'se',
    startWidth: 400,
    startHeight: 300,
    startPosition: { x: 0, y: 0 },
    dragged: { width: null, height: null },
    dx: 0,
    dy: 0,
    limits: NO_LIMITS,
    verticalDrift: 0.5,
    ...overrides,
  })
}

describe('resizeStep', () => {
  // A centered panel's layout box slides by half its growth, so the offset pays that
  // back and the edge nobody holds stays where it was.
  it('keeps the opposite edge put on a centered panel', () => {
    expect(step({ direction: 'e', dx: 60 })).toEqual({ size: { width: 460, height: null }, position: { x: 30, y: 0 } })
    expect(step({ direction: 'w', dx: -60 })).toEqual({
      size: { width: 460, height: null },
      position: { x: -30, y: 0 },
    })
    expect(step({ direction: 's', dy: 60 }).position.y).toBe(30)
    expect(step({ direction: 'n', dy: -60 }).position.y).toBe(-30)
  })

  // A top-pinned panel (`growDownward`, `align="top"`) never drifts on its own.
  it('moves a top-pinned panel by its whole growth from the north edge, and not at all from the south', () => {
    expect(step({ direction: 'n', dy: -60, verticalDrift: 0 }).position.y).toBe(-60)
    expect(step({ direction: 's', dy: 60, verticalDrift: 0 }).position.y).toBe(0)
  })

  it('stops at the limits, and the offset stops with it', () => {
    const floored = step({ direction: 's', dy: -400 })
    expect(floored.size.height).toBe(60)
    expect(floored.position.y).toBe(-120)
  })

  it('keeps the size already dragged to on the axis a direction doesn’t touch', () => {
    expect(step({ direction: 'e', dx: 10, dragged: { width: null, height: 500 } }).size.height).toBe(500)
  })
})

describe('resizeDirectionsFor', () => {
  it('exposes nothing, the side edges, or every edge and corner', () => {
    expect(resizeDirectionsFor(false)).toEqual([])
    expect(resizeDirectionsFor('horizontal')).toEqual(['w', 'e'])
    expect(resizeDirectionsFor(true)).toHaveLength(8)
  })
})

describe('declaredHeight', () => {
  it('reads the height a style string declares, and nothing when it declares none', () => {
    expect(declaredHeight('width: 500px; height: 280px')).toBe('280px')
    expect(declaredHeight('width: 500px')).toBe('')
  })
})
