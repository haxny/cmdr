/**
 * Which tracks of the Tags tab are checked. A tool (Case, Renumber, find/replace…) works on the checked tracks
 * alone, and on every track while none is checked. Paths are absolute, as in `TrackRow.path`; the set is replaced on
 * every change, never mutated, so a reader holding the old one keeps a consistent picture.
 */

/* eslint-disable svelte/prefer-svelte-reactivity -- every change builds a new Set and swaps it into `$state.raw`; no Set is mutated in place, so a plain Set is the right (and immutable-style) container */

export interface TrackSelection {
  readonly selected: ReadonlySet<string>
  toggle(path: string): void
  /** Checks (`on`) or unchecks every one of `paths`. */
  setAll(paths: string[], on: boolean): void
  /** The checked paths among `allPaths`, in that order, or `null` when none is: the tools' "every track". */
  scope(allPaths: string[]): string[] | null
  /** Forgets every checked path not in `paths` (a reload no longer lists it). */
  prune(paths: string[]): void
}

export function createTrackSelection(): TrackSelection {
  let selected = $state.raw<ReadonlySet<string>>(new Set())

  return {
    get selected() {
      return selected
    },
    toggle(path) {
      const next = new Set(selected)
      if (!next.delete(path)) next.add(path)
      selected = next
    },
    setAll(paths, on) {
      const next = new Set(selected)
      for (const path of paths) {
        if (on) next.add(path)
        else next.delete(path)
      }
      selected = next
    },
    scope(allPaths) {
      const checked = allPaths.filter((path) => selected.has(path))
      return checked.length === 0 ? null : checked
    },
    prune(paths) {
      const keep = new Set(paths)
      const next = new Set([...selected].filter((path) => keep.has(path)))
      if (next.size !== selected.size) selected = next
    },
  }
}
