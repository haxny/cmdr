import type { ActionReturn } from 'svelte/action'

/** A scroll container's box as its scroll math reads it. */
export interface ScrollBoxSize {
  /** The padding box minus any classic vertical scrollbar. */
  clientWidth: number
  /** The padding box minus any classic horizontal scrollbar. */
  clientHeight: number
  /** The border box, scrollbars included. `offsetWidth - clientWidth` is the vertical scrollbar's width. */
  offsetWidth: number
}

export interface ScrollBoxSizeParams {
  /** Runs with the element's box at mount and whenever it, or a scrollbar in it, changes size. */
  onResize: (size: ScrollBoxSize) => void
}

/**
 * Svelte action reporting a scroll container's `clientWidth` / `clientHeight` /
 * `offsetWidth`, re-read whenever a classic scrollbar appears or goes.
 *
 * Svelte's `bind:clientHeight` family observes the BORDER box, which a scrollbar
 * doesn't move: when a listing starts overflowing and a space-taking scrollbar
 * shows up, the client box shrinks but the binding keeps the old number. The file
 * lists style their scrollbars (`app-file-list.css`), which makes them classic and
 * always visible, so that stale number would put Brief's bottom row under its
 * horizontal bar and leave Full's header unpadded for its vertical one. The
 * CONTENT box is what a scrollbar takes from, so this observes that, and any
 * border-box resize moves it too.
 */
export function useScrollBoxSize(node: HTMLElement, params: ScrollBoxSizeParams): ActionReturn<ScrollBoxSizeParams> {
  let onResize = params.onResize
  const report = () => {
    onResize({ clientWidth: node.clientWidth, clientHeight: node.clientHeight, offsetWidth: node.offsetWidth })
  }

  const observer = new ResizeObserver(report)
  observer.observe(node, { box: 'content-box' })

  // Seed from the element itself: the observer's first callback lands after the
  // next layout, and an unmeasured list renders no rows for that frame.
  report()

  return {
    update(next: ScrollBoxSizeParams) {
      onResize = next.onResize
      report()
    },
    destroy() {
      observer.disconnect()
    },
  }
}
