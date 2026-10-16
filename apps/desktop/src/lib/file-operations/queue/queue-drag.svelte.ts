/**
 * Drag-to-reorder for the queue window's WAITING rows: the mouse half of ⌥↑ / ⌥↓.
 * The grabbed row follows the pointer, a drop line marks the gap it would land in,
 * and the drop sends ONE `placeQueuedOperation`. DETAILS § "Dragging to reorder".
 *
 * ❗ Pointer events, never HTML5 drag-and-drop: macOS hands drag gestures to Tauri's
 * `dragDropEnabled` before the WKWebView sees `dragstart` (same as `lib/ui/menu-controller.svelte.ts`).
 */

import { tick } from 'svelte'
import { pointerInsertionSlot, pointerReorderTarget } from '#lib/ui/menu-reorder.js'
import { slideFrom } from './queue-flip.js'

/** Below this many pixels of pointer travel, a press on the grip is a click, not a drag. */
const DRAG_THRESHOLD_PX = 4

/** How long a drop waits for the reordered list before the row lets go anyway. */
const SETTLE_TIMEOUT_MS = 1000

export interface QueueDragDeps {
  /** The waiting rows' operation ids, in line order: the only rows a drag moves among. */
  waitingIds: () => readonly string[]
  /** The row's element, which is what's translated while dragging. */
  rowElement: (id: string) => HTMLElement | undefined
  /** The scrolling list, so a wheel mid-drag keeps the row under the pointer. */
  scroller: () => HTMLElement | undefined
  /** The gesture started past the threshold: the page puts its cursor on the row. */
  onLift: (id: string) => void
  /** A drop that changes the order: put `id` at 1-based `position` among waiting rows.
   *  Resolves `true` when the backend moved it, so the row waits for the new list. */
  onDrop: (id: string, position: number) => Promise<boolean>
}

export interface QueueDrag {
  /** A press on a row's grip. Starts tracking; it becomes a drag past the threshold. */
  start: (id: string, event: PointerEvent) => void
  /** Escape mid-drag: the row goes back where it was, nothing is sent. `false` when no drag. */
  cancel: () => boolean
  /** The line's order is about to change: a dropped row stops holding its offset, and
   *  the page's FLIP (`queue-flip.ts`) carries it from where it was dropped. */
  listChanging: () => void
  /** The row being dragged, or dropped and waiting for the new list. */
  readonly draggingId: string | null
  /** True only while the pointer holds the row (not while it settles). */
  readonly active: boolean
  /** How far the row is moved from its own slot, in px. */
  readonly offsetY: number
  /** The gap the row would land in, as a WAITING-row index: `k` is the line above
   *  waiting row `k`, `waitingIds().length` is below the last. `null` for a no-op gap. */
  readonly dropSlot: number | null
}

/** jsdom has no pointer capture; WebKit does. */
function hasCapture(target: Element, pointerId: number): boolean {
  return typeof target.hasPointerCapture === 'function' && target.hasPointerCapture(pointerId)
}

export function createQueueDrag(deps: QueueDragDeps): QueueDrag {
  let draggingId = $state<string | null>(null)
  let active = $state(false)
  let offsetY = $state(0)
  let dropSlot = $state<number | null>(null)

  let pending: { id: string; pointerId: number; startY: number; startScroll: number; target: Element } | null = null
  let settleTimer: ReturnType<typeof setTimeout> | undefined

  function scrollTop(): number {
    return deps.scroller()?.scrollTop ?? 0
  }

  /** Where each waiting row's middle sits now, in client px, with the grabbed one at its own slot. */
  function midpoints(): number[] {
    return deps.waitingIds().map((id) => {
      const rect = deps.rowElement(id)?.getBoundingClientRect()
      if (!rect) return Number.NaN
      const shift = id === draggingId ? offsetY : 0
      return rect.top - shift + rect.height / 2
    })
  }

  function handleMove(event: PointerEvent): void {
    if (pending === null || event.pointerId !== pending.pointerId) return
    const travel = event.clientY - pending.startY + (scrollTop() - pending.startScroll)
    if (!active) {
      if (Math.abs(travel) < DRAG_THRESHOLD_PX) return
      clearTimeout(settleTimer)
      active = true
      draggingId = pending.id
      deps.onLift(pending.id)
    }
    offsetY = travel
    const mids = midpoints()
    const from = deps.waitingIds().indexOf(pending.id)
    if (from === -1) {
      // It started running (or left) mid-drag: it has no place in line any more.
      cancel()
      return
    }
    const slot = pointerInsertionSlot(mids, event.clientY)
    // The gaps right above and below the row itself would drop it where it is.
    dropSlot = slot === from || slot === from + 1 ? null : slot
  }

  function handleUp(event: PointerEvent): void {
    if (pending === null || event.pointerId !== pending.pointerId) return
    const { id } = pending
    const wasActive = active
    const mids = wasActive ? midpoints() : []
    const from = deps.waitingIds().indexOf(id)
    stopListening()
    active = false
    dropSlot = null
    if (!wasActive) return
    const to = from === -1 ? null : pointerReorderTarget(mids, event.clientY, from)
    if (to === null) {
      slideBack()
      return
    }
    // Hold the row where it was dropped until the reordered list arrives; the
    // page's FLIP then carries it from here into its new slot.
    settleTimer = setTimeout(slideBack, SETTLE_TIMEOUT_MS)
    void deps.onDrop(id, to + 1).then((moved) => {
      if (!moved && draggingId === id && !active) slideBack()
    })
  }

  function handleCancelEvent(event: PointerEvent): void {
    if (pending !== null && event.pointerId === pending.pointerId) cancel()
  }

  function stopListening(): void {
    const target = pending?.target
    if (pending !== null && target !== undefined && hasCapture(target, pending.pointerId)) {
      target.releasePointerCapture(pending.pointerId)
    }
    pending = null
    window.removeEventListener('pointermove', handleMove)
    window.removeEventListener('pointerup', handleUp)
    window.removeEventListener('pointercancel', handleCancelEvent)
  }

  /** The drag is over and the row drops its offset, in place: the list's order is
   *  changing under it, and the page's FLIP owns the motion. */
  function letGo(): void {
    clearTimeout(settleTimer)
    draggingId = null
    active = false
    offsetY = 0
    dropSlot = null
  }

  /** The drag is over and the row slides back into its own slot: a cancel, or a drop
   *  that didn't change the order. */
  function slideBack(): void {
    const id = draggingId
    const el = id === null ? undefined : deps.rowElement(id)
    const drawnAt = el?.getBoundingClientRect().top
    letGo()
    if (el === undefined || drawnAt === undefined) return
    void tick().then(() => {
      slideFrom(el, drawnAt)
    })
  }

  function cancel(): boolean {
    if (pending === null && draggingId === null) return false
    const wasDragging = active
    stopListening()
    slideBack()
    return wasDragging
  }

  return {
    start(id, event) {
      if (event.button !== 0 || !deps.waitingIds().includes(id)) return
      // A second press while a drop settles takes over from it.
      cancel()
      const target = event.currentTarget instanceof Element ? event.currentTarget : document.body
      // Capture keeps the moves coming when the pointer leaves the window mid-drag. It's a
      // nicety: WebKit refuses it (`NotFoundError`) for a pointer that's no longer down, and
      // the window listeners still carry the drag without it.
      try {
        target.setPointerCapture(event.pointerId)
      } catch {
        // No capture: moves outside the window are lost, nothing else changes.
      }
      pending = { id, pointerId: event.pointerId, startY: event.clientY, startScroll: scrollTop(), target }
      window.addEventListener('pointermove', handleMove)
      window.addEventListener('pointerup', handleUp)
      window.addEventListener('pointercancel', handleCancelEvent)
    },
    cancel,
    listChanging() {
      if (draggingId !== null && !active) letGo()
    },
    get draggingId() {
      return draggingId
    },
    get active() {
      return active
    },
    get offsetY() {
      return offsetY
    },
    get dropSlot() {
      return dropSlot
    },
  }
}
