/**
 * FLIP for the queue's rows: when the line changes order, each row slides from where it
 * was drawn to its new slot instead of jumping. DETAILS § "Dragging to reorder".
 *
 * ❗ Not Svelte's `animate:flip`: it measures the row's END position before the page's
 * reactive styles run, so a dropped row still wearing its drag offset would animate from
 * the wrong place and then jump. Here the page measures in `$effect.pre` (before the DOM
 * changes, so a dragged row is measured where it was dropped) and plays after `tick()`.
 */

const SLIDE_MS = 180

/** Where each row (`[data-operation-id]`) under `container` is drawn now, by operation id. */
export function measureRowTops(container: HTMLElement): Map<string, number> {
  const tops = new Map<string, number>()
  for (const el of container.querySelectorAll<HTMLElement>('[data-operation-id]')) {
    const id = el.dataset.operationId
    if (id !== undefined) tops.set(id, el.getBoundingClientRect().top)
  }
  return tops
}

/** Slides every row that moved since `before` from its old place into its new one. */
export function slideRowsFrom(container: HTMLElement, before: ReadonlyMap<string, number>): void {
  for (const el of container.querySelectorAll<HTMLElement>('[data-operation-id]')) {
    const from = before.get(el.dataset.operationId ?? '')
    if (from !== undefined) slideFrom(el, from)
  }
}

/** Slides one element from client-y `fromTop` to where it's laid out now. */
export function slideFrom(el: HTMLElement, fromTop: number): void {
  const delta = fromTop - el.getBoundingClientRect().top
  if (Math.abs(delta) < 0.5 || prefersReducedMotion() || typeof el.animate !== 'function') return
  el.animate([{ transform: `translateY(${String(delta)}px)` }, { transform: 'none' }], {
    duration: SLIDE_MS,
    easing: 'cubic-bezier(0.2, 0, 0, 1)',
  })
}

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
}
