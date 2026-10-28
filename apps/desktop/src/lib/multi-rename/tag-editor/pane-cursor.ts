/**
 * Where a cursor key takes a Tags window pane's cursor, file-pane style: the bare keys move it (`list-cursor.ts`),
 * and the same keys with ⇧ also extend the selection over the rows passed (the Tracks pane). Pure.
 */

import { listCursorMoveOf, nextListCursor, type ListCursorMove } from '#lib/ui/list-cursor.js'

export interface PaneCursorMove {
  to: number
  /** ⇧ was held: the Tracks pane fills the selection from the old row to `to`. */
  extend: boolean
  /** The move hit an end of the list, so the landing row is part of the fill (`handleShiftKeyboardNavigation`). */
  overflow: boolean
}

export interface PaneCursorTarget {
  count: number
  from: number
  visibleRows: number
}

/** The cursor move a keypress asks for, or `null` for any other key (and for a list with no rows). */
export function paneCursorMove(event: KeyboardEvent, target: PaneCursorTarget): PaneCursorMove | null {
  const extend = event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey
  const move = listCursorMoveOf({
    key: event.key,
    isComposing: event.isComposing,
    altKey: event.altKey,
    metaKey: event.metaKey,
    ctrlKey: event.ctrlKey,
    shiftKey: extend ? false : event.shiftKey,
  })
  if (move === null || target.count === 0) return null
  const from = Math.max(0, Math.min(target.from, target.count - 1))
  const to = nextListCursor(move, { count: target.count, from, visibleRows: target.visibleRows })
  if (to < 0) return null
  return { to, extend, overflow: overshoots(move, from, target) }
}

function overshoots(move: ListCursorMove, from: number, target: PaneCursorTarget): boolean {
  const last = target.count - 1
  switch (move.kind) {
    case 'edge':
      return true
    case 'step':
      return from + move.delta < 0 || from + move.delta > last
    case 'page': {
      const intended = from + move.delta * Math.max(1, target.visibleRows - 1)
      return intended < 0 || intended > last
    }
  }
}

/** How many rows of `rowHeight` fit in `list`, at least one: what a page key moves by (less one). */
export function visibleRowCount(list: HTMLElement | undefined, rowHeight: number): number {
  if (!list || rowHeight <= 0 || list.clientHeight <= 0) return 1
  return Math.max(1, Math.floor(list.clientHeight / rowHeight))
}

/** Where the in-place editor sits, in the scroller's coordinates; `height` `null` lets a multiline box size itself. */
export interface EditorPlace {
  top: number
  left: number
  width: number
  height: number | null
}

/** The narrowest the editor gets, so a squeezed column still leaves room to type. */
const EDITOR_MIN_WIDTH = 40

/** Where the in-place editor sits: over `cell`, in the coordinates of the scrolling `container`. */
export function editorBox(container: HTMLElement, cell: HTMLElement, height: number | null): EditorPlace {
  const outer = container.getBoundingClientRect()
  const inner = cell.getBoundingClientRect()
  return {
    top: inner.top - outer.top + container.scrollTop,
    left: inner.left - outer.left + container.scrollLeft,
    width: Math.max(inner.width, EDITOR_MIN_WIDTH),
    height,
  }
}

/**
 * Calls `onResize` whenever `el` changes size (a window resize reflows the flexing column), so an open editor can
 * follow its cell. Returns the cleanup.
 */
export function onResize(el: HTMLElement, callback: () => void): () => void {
  const observer = new ResizeObserver(() => {
    callback()
  })
  observer.observe(el)
  return () => {
    observer.disconnect()
  }
}

/** Scrolls row `index` of `list` into view after a key moved the cursor there. */
export function revealRow(list: HTMLElement | undefined, index: number): void {
  const row = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"]`)
  // jsdom has no `scrollIntoView`; the tests run there.
  if (row && typeof row.scrollIntoView === 'function') row.scrollIntoView({ block: 'nearest' })
}
