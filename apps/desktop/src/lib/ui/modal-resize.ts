/**
 * The geometry of resizing a `ModalDialog` by an edge or a corner, as pure
 * functions: which bands a dialog exposes, the size limits a drag stays inside,
 * and the next size and offset for a pointer delta. `ModalDialog.svelte` keeps
 * only the listener plumbing. `../ui/DETAILS.md` § ModalDialog.
 */

/**
 * Edges first, corners last: they're siblings in the panel, so the later ones
 * win the hit test where a corner overlaps the two edges it joins.
 */
export const ALL_RESIZE_DIRECTIONS = ['n', 's', 'w', 'e', 'nw', 'ne', 'sw', 'se'] as const
export type ResizeDirection = (typeof ALL_RESIZE_DIRECTIONS)[number]

/** The bands a dialog exposes: none, the two side edges, or every edge and corner. */
export function resizeDirectionsFor(resizable: boolean | 'horizontal'): readonly ResizeDirection[] {
  if (resizable === false) return []
  return resizable === 'horizontal' ? (['w', 'e'] as const) : ALL_RESIZE_DIRECTIONS
}

export interface Position {
  x: number
  y: number
}

/** The size the user dragged the panel to. `null` on an axis means "still content- or `containerStyle`-driven". */
export interface DraggedSize {
  width: number | null
  height: number | null
}

export interface ResizeLimits {
  minWidth: number
  maxWidth: number
  minHeight: number
  maxHeight: number
}

/** Resolves a computed `min-*` / `max-*` to a number; `none` and the empty string mean "no limit". */
export function cssLength(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value)
  return Number.isNaN(parsed) ? fallback : parsed
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

/**
 * Height the panel can't be dragged below: its chrome, plus enough body to read
 * a line or two. The chrome is measured rather than assumed, because a wrapped
 * title or a footer with a leading control is taller than a one-line one.
 */
export function minimumPanelHeight(el: HTMLElement): number {
  const chrome = [...el.querySelectorAll<HTMLElement>('.dialog-title-bar, .modal-footer')].reduce(
    (total, part) => total + part.offsetHeight,
    0,
  )
  const readableBody = 60
  return chrome + readableBody
}

/** The limits a drag of `el` stays inside: its computed `min-*` / `max-*`, and the height floor. */
export function resizeLimitsOf(el: HTMLElement): ResizeLimits {
  const styles = getComputedStyle(el)
  return {
    minWidth: cssLength(styles.minWidth, 0),
    maxWidth: cssLength(styles.maxWidth, Number.POSITIVE_INFINITY),
    minHeight: Math.max(cssLength(styles.minHeight, 0), minimumPanelHeight(el)),
    maxHeight: cssLength(styles.maxHeight, Number.POSITIVE_INFINITY),
  }
}

export interface ResizeStepInput {
  direction: ResizeDirection
  /** The panel's size when the drag started. */
  startWidth: number
  startHeight: number
  /** The drag offset when the drag started. */
  startPosition: Position
  /** The size already dragged to, kept on an axis this direction doesn't touch. */
  dragged: DraggedSize
  /** How far the pointer moved since the drag started. */
  dx: number
  dy: number
  limits: ResizeLimits
  /** How much of a vertical growth slides the panel's layout box: 0.5 centered, 0 with its top edge pinned. */
  verticalDrift: number
}

/**
 * The next size and offset for one pointer move, keeping the OPPOSITE edge where
 * it is.
 *
 * That last part is the whole trick, because the panel is centered rather than
 * absolutely placed: widening it by `growth` also slides its layout box left by
 * `growth / 2`, so the drag offset has to pay that back. Vertically the share
 * depends on the alignment (`verticalDrift`): a centered panel drifts by half its
 * growth, while a `growDownward` or `align="top"` panel has its top edge pinned
 * and drifts by none, so dragging its top edge has to move the whole growth.
 */
export function resizeStep(input: ResizeStepInput): { size: DraggedSize; position: Position } {
  const { direction, startWidth, startHeight, startPosition, dx, dy, limits, verticalDrift } = input
  const position = { ...startPosition }
  let { width, height } = input.dragged

  if (direction.includes('w') || direction.includes('e')) {
    const towardsEast = direction.includes('e')
    width = clamp(startWidth + (towardsEast ? dx : -dx), limits.minWidth, limits.maxWidth)
    const growth = width - startWidth
    position.x = startPosition.x + (towardsEast ? growth / 2 : -growth / 2)
  }
  if (direction.includes('n') || direction.includes('s')) {
    const towardsSouth = direction.includes('s')
    height = clamp(startHeight + (towardsSouth ? dy : -dy), limits.minHeight, limits.maxHeight)
    const growth = height - startHeight
    position.y = startPosition.y + growth * (towardsSouth ? verticalDrift : -(1 - verticalDrift))
  }

  return { size: { width, height }, position }
}

/** The height a style string declares, `''` for none: parsed by the browser, never by hand. */
export function declaredHeight(style: string): string {
  const probe = document.createElement('div')
  probe.style.cssText = style
  return probe.style.height
}
