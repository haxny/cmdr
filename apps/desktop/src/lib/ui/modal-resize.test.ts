import { describe, it, expect } from 'vitest'
import { declaredStyle, panelMinWidth, resizeDirectionsFor, resizeStep, type ResizeStepInput } from './modal-resize'

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

describe('declaredStyle', () => {
  it('reads what a style string declares, and nothing when it declares none', () => {
    expect(declaredStyle('width: 500px; height: 280px', 'height')).toBe('280px')
    expect(declaredStyle('width: 500px', 'height')).toBe('')
    expect(declaredStyle('width: 500px; min-width: 460px', 'min-width')).toBe('460px')
  })
})

describe('panelMinWidth', () => {
  it('takes the larger of the caller’s floor and the footer’s button row', () => {
    expect(panelMinWidth('width: 580px; min-width: 580px', true, 640)).toBe('max(580px, 640px)')
    expect(panelMinWidth('width: 400px', false, 430)).toBe('430px')
  })

  it('gives a resizable panel its own floor when the caller sets none', () => {
    expect(panelMinWidth('width: 400px', true, 0)).toBe('360px')
    expect(panelMinWidth('width: 400px', true, 430)).toBe('max(360px, 430px)')
  })

  it('leaves a footerless panel the caller’s floor, or none', () => {
    expect(panelMinWidth('min-width: 460px; max-width: 560px', false, 0)).toBe('460px')
    expect(panelMinWidth('width: 400px', false, 0)).toBe('')
  })
})
