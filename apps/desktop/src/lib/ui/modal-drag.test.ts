import { describe, it, expect } from 'vitest'
import { clampDragOffset, clampPointToArea, MIN_VISIBLE_TITLE_PX, type DragFrame } from './modal-drag'

/** A 1000×700 overlay starting 40px down (below the window's title bar), a 400-wide panel centered in it. */
const FRAME: DragFrame = {
  area: { left: 0, top: 40, right: 1000, bottom: 740 },
  home: { x: 300, y: 240 },
  panelWidth: 400,
  titleBarHeight: 44,
}

describe('clampDragOffset', () => {
  it('leaves an offset that keeps the title bar on screen alone', () => {
    expect(clampDragOffset({ x: 120, y: -80 }, FRAME)).toEqual({ x: 120, y: -80 })
  })

  // The bug this exists for: dragged up past the overlay, the title bar hid under the
  // window's own title bar and nothing was left to grab.
  it('stops the panel top at the overlay top', () => {
    expect(clampDragOffset({ x: 0, y: -500 }, FRAME).y).toBe(40 - 240)
  })

  it('keeps the whole title bar height above the bottom', () => {
    expect(clampDragOffset({ x: 0, y: 900 }, FRAME).y).toBe(740 - 44 - 240)
  })

  it('keeps a grabbable strip of the title bar inside each side', () => {
    const left = clampDragOffset({ x: -2000, y: 0 }, FRAME).x
    expect(300 + left + 400).toBe(MIN_VISIBLE_TITLE_PX)
    const right = clampDragOffset({ x: 2000, y: 0 }, FRAME).x
    expect(300 + right).toBe(1000 - MIN_VISIBLE_TITLE_PX)
  })

  it('keeps a panel narrower than the strip fully inside', () => {
    const narrow = { ...FRAME, panelWidth: 60 }
    expect(narrow.home.x + clampDragOffset({ x: -2000, y: 0 }, narrow).x).toBe(0)
    expect(narrow.home.x + clampDragOffset({ x: 2000, y: 0 }, narrow).x + 60).toBe(1000)
  })

  // A window shorter than the title bar can't satisfy both limits; the top one is what
  // keeps the title readable, so it wins.
  it('favors the top limit when the window is too short for both', () => {
    const short = { ...FRAME, area: { ...FRAME.area, bottom: 60 } }
    expect(clampDragOffset({ x: 0, y: 300 }, short).y).toBe(40 - 240)
  })
})

describe('clampPointToArea', () => {
  it('pulls a point outside the area onto its nearest edge', () => {
    expect(clampPointToArea({ x: -10, y: 10 }, FRAME.area)).toEqual({ x: 0, y: 40 })
    expect(clampPointToArea({ x: 1200, y: 900 }, FRAME.area)).toEqual({ x: 1000, y: 740 })
    expect(clampPointToArea({ x: 500, y: 300 }, FRAME.area)).toEqual({ x: 500, y: 300 })
  })
})
