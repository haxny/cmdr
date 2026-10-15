/**
 * Keeping the toast stack off the row the keyboard is on (WCAG 2.4.11, focus not obscured).
 *
 * The stack sits at the top right, over the right pane's first rows. While it would cover the
 * FOCUSED element's cursor row (the element named by the focused element's
 * `aria-activedescendant`, the file list's model, or the focused element itself), it moves to
 * the bottom right, as long as the bottom is clear. It stays wherever it's clear rather than
 * snapping back the moment the cursor moves on, so arrowing down a list doesn't bounce it, and
 * the container puts it back at the top once the last toast goes. In the common case (the cursor
 * isn't under a toast) nothing moves. At the bottom it sits above any window chrome marked
 * `data-toast-bottom-inset` (the function key bar), measured, so a hidden bar costs nothing and a
 * larger text size moves it with the bar.
 *
 * Event-driven, and only while toasts are up: focus moves, the focused element's
 * `aria-activedescendant` changes, scrolling, window resizes, and the stack's own size. One
 * measurement per frame at most.
 */

export type ToastEdge = 'top' | 'bottom'

export interface Box {
  top: number
  bottom: number
  left: number
  right: number
}

/** What marks window chrome along the bottom edge that the stack must sit above. */
const BOTTOM_CHROME_SELECTOR = '[data-toast-bottom-inset]'

/** How far up from the window's bottom edge the bottom chrome reaches; 0 when none is rendered. */
export function bottomChromeInset(viewportHeight: number): number {
  let inset = 0
  for (const element of document.querySelectorAll(BOTTOM_CHROME_SELECTOR)) {
    inset = Math.max(inset, viewportHeight - element.getBoundingClientRect().top)
  }
  return Math.max(0, Math.round(inset))
}

/**
 * Where the stack would sit at each edge: the measured one, and its mirror at the same gap
 * from the other edge. At the bottom, the gap counts from the top of the bottom chrome.
 */
export function placementRects(
  stack: Box,
  current: ToastEdge,
  viewportHeight: number,
  bottomInset = 0,
): Record<ToastEdge, Box> {
  const height = stack.bottom - stack.top
  const floor = viewportHeight - bottomInset
  const gap = current === 'top' ? stack.top : floor - stack.bottom
  const at = (top: number): Box => ({ top, bottom: top + height, left: stack.left, right: stack.right })
  return { top: at(gap), bottom: at(floor - gap - height) }
}

function overlaps(a: Box, b: Box): boolean {
  return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/** Stay where the row is clear; move only when the other edge is clear and this one isn't. */
export function pickToastEdge({
  current,
  rects,
  keepClear,
}: {
  current: ToastEdge
  rects: Record<ToastEdge, Box>
  keepClear: Box | null
}): ToastEdge {
  if (!keepClear || !overlaps(rects[current], keepClear)) return current
  const other: ToastEdge = current === 'top' ? 'bottom' : 'top'
  return overlaps(rects[other], keepClear) ? current : other
}

/** The element the keyboard is on: the focused element's active descendant, else the focused element. */
function keyboardTarget(stack: HTMLElement): Element | null {
  const active = document.activeElement
  if (!active || active === document.body || stack.contains(active)) return null
  const descendantId = active.getAttribute('aria-activedescendant')
  if (descendantId) return document.getElementById(descendantId)
  return active
}

/**
 * Watches what the stack must keep clear and reports each change of edge or bottom inset. The
 * first call reports only when something differs from the top with no inset. Returns the stop,
 * and `remeasure` for a change no event announces (the function key bar shown or hidden).
 */
export function watchToastPlacement(
  stack: HTMLElement,
  onPlacement: (placement: { edge: ToastEdge; bottomInset: number }) => void,
): { stop: () => void; remeasure: () => void } {
  let edge: ToastEdge = 'top'
  let bottomInset = 0
  let frame: number | null = null

  const measure = (): void => {
    frame = null
    const target = keyboardTarget(stack)
    const inset = bottomChromeInset(window.innerHeight)
    const next = pickToastEdge({
      current: edge,
      // Measured against the inset in force now, which is where the stack actually sits.
      rects: placementRects(stack.getBoundingClientRect(), edge, window.innerHeight, bottomInset),
      keepClear: target?.getBoundingClientRect() ?? null,
    })
    if (next === edge && inset === bottomInset) return
    edge = next
    bottomInset = inset
    onPlacement({ edge, bottomInset })
  }
  const schedule = (): void => {
    frame ??= requestAnimationFrame(measure)
  }

  // The focused element's cursor moves by its `aria-activedescendant`; follow whichever has focus.
  const cursorWatch = new MutationObserver(schedule)
  const followFocus = (): void => {
    cursorWatch.disconnect()
    const active = document.activeElement
    if (active && active !== document.body) {
      cursorWatch.observe(active, { attributes: true, attributeFilter: ['aria-activedescendant'] })
    }
    schedule()
  }
  const sizeWatch = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(schedule)
  sizeWatch?.observe(stack)

  document.addEventListener('focusin', followFocus)
  document.addEventListener('scroll', schedule, { capture: true, passive: true })
  window.addEventListener('resize', schedule)
  followFocus()

  return {
    stop: () => {
      if (frame !== null) cancelAnimationFrame(frame)
      cursorWatch.disconnect()
      sizeWatch?.disconnect()
      document.removeEventListener('focusin', followFocus)
      document.removeEventListener('scroll', schedule, { capture: true })
      window.removeEventListener('resize', schedule)
    },
    remeasure: schedule,
  }
}
