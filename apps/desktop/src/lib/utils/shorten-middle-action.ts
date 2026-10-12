import type { ActionReturn } from 'svelte/action'
import { shortenMiddle, createPretextMeasure } from './shorten-middle'
import { tooltip } from '#lib/tooltip/tooltip.js'

export interface ShortenMiddleParams {
  text: string
  preferBreakAt?: string
  startRatio?: number
  /**
   * When true, the tooltip appears only when truncation actually happened (so short,
   * fully-visible text doesn't trigger a redundant tooltip). Defaults to false: the
   * full text is on hover whatever the width.
   */
  tooltipWhenTruncated?: boolean
  /**
   * Hover text that says MORE than `text` (a transfer tree row shows its name and hovers
   * its full relative path). Shown whatever the width, so it overrides
   * `tooltipWhenTruncated`.
   */
  tooltip?: string
}

// Shared across all action instances so the dynamic import runs at most once.
let pretextPromise: Promise<typeof import('@chenglou/pretext')> | null = null

function loadPretext(): Promise<typeof import('@chenglou/pretext')> {
  if (!pretextPromise) {
    pretextPromise = import('@chenglou/pretext')
  }
  return pretextPromise
}

/**
 * Reads the CSS font string from a DOM element. Falls back to constructing
 * it from fontSize + fontFamily when `getComputedStyle().font` is empty.
 */
function readFont(node: HTMLElement): string {
  const style = getComputedStyle(node)
  if (style.font) return style.font
  return `${style.fontSize} ${style.fontFamily}`
}

/**
 * Svelte action that truncates text in the middle to fit its container width.
 * Uses `@chenglou/pretext` for pixel-accurate measurement (loaded async).
 * Before pretext loads, the full text is shown with CSS `text-overflow: ellipsis`.
 *
 * The full text is always reachable on hover, through the HOUSE tooltip rather than a
 * native `title`: the two have different delays and different chrome, and this action
 * feeds pane rows, dialog rows, and result lists alike, which would otherwise hover
 * three different ways.
 */
export function useShortenMiddle(node: HTMLElement, params: ShortenMiddleParams): ActionReturn<ShortenMiddleParams> {
  let measureWidth: ((text: string) => number) | null = null
  let currentText = params.text

  // CSS fallback while pretext is loading
  node.style.overflow = 'hidden'
  node.style.textOverflow = 'ellipsis'
  node.style.whiteSpace = 'nowrap'
  node.textContent = params.text
  function hoverText(truncated: boolean): string {
    if (params.tooltip !== undefined) return params.tooltip
    return !params.tooltipWhenTruncated || truncated ? currentText : ''
  }

  // Nothing is truncated yet, so `tooltipWhenTruncated` starts out with nothing to say.
  const tip = tooltip(node, hoverText(false))

  function applyTooltip(truncated: boolean): void {
    tip.update?.(hoverText(truncated))
  }

  // What the last truncation was computed for and what it wrote. A width transition resizes every
  // cell each frame for 300 ms, and a write whose text comes out the same still costs a DOM mutation
  // plus style and layout, so both the re-measure and the write are skipped when nothing moved.
  let lastWidth = -1
  let lastInput = ''
  let lastResult: string | null = null

  function truncate() {
    if (!measureWidth) return
    const width = node.clientWidth
    if (width <= 0) {
      // Nothing to measure against (hidden, or not laid out yet): show the whole text and let
      // the first real width truncate it. Bailing without a write left a recycled row showing
      // the text it had before.
      if (lastResult !== currentText) {
        lastWidth = -1
        lastResult = currentText
        node.textContent = currentText
        applyTooltip(false)
      }
      return
    }
    if (width === lastWidth && currentText === lastInput) return
    const result = shortenMiddle(currentText, width, measureWidth, {
      preferBreakAt: params.preferBreakAt,
      startRatio: params.startRatio,
    })
    lastWidth = width
    lastInput = currentText
    if (result === lastResult) return
    lastResult = result
    node.textContent = result
    applyTooltip(result !== currentText)
  }

  // Load pretext, then switch from CSS fallback to pixel-accurate truncation
  loadPretext()
    .then((pretext) => {
      const font = readFont(node)
      measureWidth = createPretextMeasure(font, pretext)
      truncate()
    })
    .catch(() => {
      // Pretext unavailable; CSS text-overflow: ellipsis remains as fallback
    })

  const observer = new ResizeObserver(() => {
    truncate()
  })
  observer.observe(node)

  return {
    update(newParams: ShortenMiddleParams) {
      const textChanged = newParams.text !== currentText
      const tooltipChanged = newParams.tooltip !== params.tooltip
      params = newParams
      if (!textChanged) {
        if (tooltipChanged) applyTooltip(lastResult !== null && lastResult !== currentText)
        return
      }
      currentText = newParams.text
      if (measureWidth) {
        truncate()
      } else {
        node.textContent = currentText
        // Pre-pretext: nothing measured yet, so "truncated" is unknowable and false.
        applyTooltip(false)
      }
    },
    destroy() {
      observer.disconnect()
      tip.destroy?.()
    },
  }
}
