/**
 * Where a `ModalDialog` may be dragged, as pure geometry: the title bar always stays
 * reachable inside the overlay, so a dialog dragged against an edge can be grabbed again.
 * The pointer roams free; only the panel stops, the way a macOS window stops at the menu
 * bar. `../ui/DETAILS.md` § ModalDialog.
 */

import type { Position } from './modal-resize'

/** How much of the title bar stays on screen past a side edge: enough to grab without hunting. */
export const MIN_VISIBLE_TITLE_PX = 100

export interface DragArea {
  left: number
  top: number
  right: number
  bottom: number
}

export interface DragFrame {
  /** The overlay's rect: the window below its title bar. */
  area: DragArea
  /** Where the panel's top-left sits at offset (0, 0), in the same coordinates as `area`. */
  home: Position
  panelWidth: number
  titleBarHeight: number
}

/**
 * `offset`, pulled back just far enough that the whole title bar stays below the overlay's
 * top, its full height stays above the bottom, and `MIN_VISIBLE_TITLE_PX` of it stays inside
 * each side. Where the window is too small for both limits of an axis, the top and the left
 * win: those are where the title and the close button start.
 *
 * Clamps the RAW offset (cursor minus grab point) on every move, never the last clamped one,
 * so the grab point doesn't drift: the panel picks the pointer back up where it stopped.
 */
export function clampDragOffset(offset: Position, frame: DragFrame): Position {
  const { area, home, panelWidth, titleBarHeight } = frame
  const visible = Math.min(MIN_VISIBLE_TITLE_PX, panelWidth)
  const minX = area.left + visible - panelWidth - home.x
  const maxX = area.right - visible - home.x
  const minY = area.top - home.y
  const maxY = area.bottom - titleBarHeight - home.y
  return {
    x: Math.max(Math.min(offset.x, maxX), minX),
    y: Math.max(Math.min(offset.y, maxY), minY),
  }
}

/**
 * Reads the frame off the live DOM. `null` when the overlay has no size (jsdom, or a
 * window mid-teardown): with no layout there is nothing to clamp against.
 *
 * `home` comes from the panel's rect minus the offset it's rendered at right now (its
 * inline `left` / `top`), not the offset in state, which may not have been written yet.
 */
export function dragFrameOf(overlay: HTMLElement, panel: HTMLElement, titleBar: HTMLElement): DragFrame | null {
  const area = overlay.getBoundingClientRect()
  if (area.width === 0 || area.height === 0) return null
  const rect = panel.getBoundingClientRect()
  return {
    area: { left: area.left, top: area.top, right: area.right, bottom: area.bottom },
    home: {
      x: rect.left - (parseFloat(panel.style.left) || 0),
      y: rect.top - (parseFloat(panel.style.top) || 0),
    },
    panelWidth: rect.width,
    titleBarHeight: titleBar.getBoundingClientRect().height,
  }
}

/** `point` pulled inside `area`: a resize edge can't be dragged past the window's edges. */
export function clampPointToArea(point: Position, area: DragArea): Position {
  return {
    x: Math.max(Math.min(point.x, area.right), area.left),
    y: Math.max(Math.min(point.y, area.bottom), area.top),
  }
}
