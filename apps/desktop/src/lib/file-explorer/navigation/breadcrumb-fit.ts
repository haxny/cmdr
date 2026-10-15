/**
 * Fits the location bar's path into its width. Every segment stays on screen: when the whole
 * path doesn't fit, the widest segments shrink to one shared width cap, each keeping its start
 * and end around a middle `…`, so short ancestors (`private`, `tmp`) stay whole and the long
 * one (a UUID, a mangled project dir) gives way. Pure: the caller measures text.
 */

const ELLIPSIS = '…'

/** Whether a `/` separator renders before segment `index` (never before the root marker `''`). */
export function hasSeparatorBefore(index: number, text: string): boolean {
  return index > 0 && text !== ''
}

/**
 * The widest `start…end` of `text` that fits `maxWidth`, the odd character going to the start.
 * The text itself when it fits; a lone `…` when not even one character each side does.
 */
export function middleTruncate(text: string, maxWidth: number, measure: (text: string) => number): string {
  if (measure(text) <= maxWidth) return text
  // Code points, so a surrogate pair (an emoji) is never cut in half.
  const chars = Array.from(text)
  const candidate = (kept: number): string =>
    chars.slice(0, Math.ceil(kept / 2)).join('') + ELLIPSIS + chars.slice(chars.length - Math.floor(kept / 2)).join('')
  // Binary search for the most characters kept; one each side is the least worth showing.
  let low = 2
  let high = chars.length - 1
  let best = ELLIPSIS
  while (low <= high) {
    const mid = Math.floor((low + high) / 2)
    const shown = candidate(mid)
    if (measure(shown) <= maxWidth) {
      best = shown
      low = mid + 1
    } else {
      high = mid - 1
    }
  }
  return best
}

export interface FitBreadcrumbInput {
  /** The segments' texts, in order; `''` is the root marker. */
  texts: string[]
  /** The width the path may take, in px. `0` (not measured yet) leaves the texts as they are. */
  availableWidth: number
  /** The width of one rendered `/` separator. */
  separatorWidth: number
  measure: (text: string) => number
}

/** The text to show for each segment: the segment's own, or a middle-truncated form of it. */
export function fitBreadcrumbSegments({
  texts,
  availableWidth,
  separatorWidth,
  measure,
}: FitBreadcrumbInput): string[] {
  if (availableWidth <= 0) return texts
  const widths = texts.map(measure)
  const separators = texts.filter((text, i) => hasSeparatorBefore(i, text)).length
  let budget = availableWidth - separators * separatorWidth
  if (widths.reduce((sum, w) => sum + w, 0) <= budget) return texts

  // Water-filling: walk the widths narrowest first, letting each one through whole while it's
  // within an even share of what's left. The first that isn't sets the cap for it and all wider.
  const sorted = [...widths].sort((a, b) => a - b)
  let cap = 0
  for (let i = 0; i < sorted.length; i++) {
    const share = budget / (sorted.length - i)
    if (sorted[i] > share) {
      cap = share
      break
    }
    budget -= sorted[i]
  }
  return texts.map((text, i) => (widths[i] > cap ? middleTruncate(text, cap, measure) : text))
}
