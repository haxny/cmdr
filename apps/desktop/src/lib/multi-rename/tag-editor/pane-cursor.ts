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

/** Where the in-place editor sits: over `cell`, in the coordinates of the scrolling `container`. */
export function editorBox(container: HTMLElement, cell: HTMLElement, height: number | null): string {
  const outer = container.getBoundingClientRect()
  const inner = cell.getBoundingClientRect()
  const top = inner.top - outer.top + container.scrollTop
  const left = inner.left - outer.left + container.scrollLeft
  const sized = height === null ? '' : ` height: ${String(height)}px;`
  return `top: ${String(top)}px; left: ${String(left)}px; width: ${String(Math.max(inner.width, 40))}px;${sized}`
}

/** Scrolls row `index` of `list` into view after a key moved the cursor there. */
export function revealRow(list: HTMLElement | undefined, index: number): void {
  const row = list?.querySelector<HTMLElement>(`[data-index="${String(index)}"]`)
  // jsdom has no `scrollIntoView`; the tests run there.
  if (row && typeof row.scrollIntoView === 'function') row.scrollIntoView({ block: 'nearest' })
}
