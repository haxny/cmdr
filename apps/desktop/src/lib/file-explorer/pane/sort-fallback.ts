import { tString } from '#lib/intl/messages.svelte.js'
import type { SortColumn } from '../types'

/**
 * Why the rows follow another column than the one asked for, or `null` when they
 * follow it. Only a date can fall back (a location that doesn't record it sorts
 * by Modified, `EffectiveSort::resolve` in `cmdr-listing`), so every other pair
 * answers `null`. The one sentence the fallback toast says.
 */
export function sortFallbackMessage(requested: SortColumn, effective: SortColumn): string | null {
  if (requested === effective) return null
  if (requested === 'created') return tString('fileExplorer.sortFallback.created')
  if (requested === 'added') return tString('fileExplorer.sortFallback.added')
  return null
}

/**
 * The same sentence as a sort header's `fallbackNote`: `undefined` when the rows
 * follow the requested column, or when either side is unknown (a ranked search
 * pane, no listing landed yet).
 */
export function sortFallbackNote(requested: SortColumn | null, effective: SortColumn | null): string | undefined {
  if (requested === null || effective === null) return undefined
  return sortFallbackMessage(requested, effective) ?? undefined
}
