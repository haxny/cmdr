import type { ActionReturn } from 'svelte/action'
import { fitBreadcrumbSegments, segmentLayoutFloor } from './breadcrumb-fit'

/**
 * Slack between canvas text metrics and the laid-out DOM (subpixel rounding), so a path the
 * fit says fits never trips the container's own end clip.
 */
const MEASURE_SLACK_PX = 2

export interface BreadcrumbFit {
  /** The text to show for each segment, index for index with the texts it was given. */
  readonly shown: string[]
  /**
   * Action for the path's container: tracks its width and font (a resize, or a UI font size
   * change, which resizes it too), and keeps a keyboard-focused segment in view.
   */
  observe: (node: HTMLElement) => ActionReturn
}

/** The location bar's path, fitted into its width by `fitBreadcrumbSegments`. */
export function createBreadcrumbFit(getTexts: () => string[]): BreadcrumbFit {
  let width = $state(0)
  let font = $state('')
  let context: CanvasRenderingContext2D | null = null

  const shown = $derived.by((): string[] => {
    const texts = getTexts()
    const ctx = context
    if (!ctx || !font || width <= 0) return texts
    ctx.font = font
    const measure = (text: string): number => ctx.measureText(text).width
    return fitBreadcrumbSegments({
      texts,
      availableWidth: Math.max(1, width - MEASURE_SLACK_PX),
      separatorWidth: measure('/'),
      measure,
      floor: segmentLayoutFloor,
    })
  })

  function observe(node: HTMLElement): ActionReturn {
    context ??= document.createElement('canvas').getContext('2d')
    const observer = new ResizeObserver((entries) => {
      font = getComputedStyle(node).font
      width = entries[entries.length - 1].contentRect.width
    })
    observer.observe(node)

    // The fit keeps every segment on screen down to a lone `…` each; past that the container
    // clips, so a segment ⌃F5 and the arrows walk onto must scroll into view, and the path goes
    // back to its start once focus leaves it.
    const handleFocusIn = (event: FocusEvent): void => {
      if (event.target instanceof HTMLElement) event.target.scrollIntoView({ block: 'nearest', inline: 'nearest' })
    }
    const handleFocusOut = (event: FocusEvent): void => {
      if (!(event.relatedTarget instanceof Node && node.contains(event.relatedTarget))) node.scrollLeft = 0
    }
    node.addEventListener('focusin', handleFocusIn)
    node.addEventListener('focusout', handleFocusOut)

    return {
      destroy() {
        observer.disconnect()
        node.removeEventListener('focusin', handleFocusIn)
        node.removeEventListener('focusout', handleFocusOut)
      },
    }
  }

  return {
    get shown() {
      return shown
    },
    observe,
  }
}
