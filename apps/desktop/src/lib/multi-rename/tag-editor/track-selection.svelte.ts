/**
 * Which tracks of the Tags tab are checked. A tool (Case, Renumber, find/replace…) works on the checked tracks
 * alone, and on every track while none is checked. Paths are absolute, as in `TrackRow.path`; the set is replaced on
 * every change, never mutated, so a reader holding the old one keeps a consistent picture.
 */

/* eslint-disable svelte/prefer-svelte-reactivity -- every change builds a new Set and swaps it into `$state.raw`; no Set is mutated in place, so a plain Set is the right (and immutable-style) container */

/** What a tool works on: every track (none checked), the checked ones, or nothing because they all vanished. */
export type ToolScope = { kind: 'all' } | { kind: 'some'; paths: string[] } | { kind: 'gone' }

export interface TrackSelection {
  readonly selected: ReadonlySet<string>
  toggle(path: string): void
  /** Checks (`on`) or unchecks every one of `paths`. */
  setAll(paths: string[], on: boolean): void
  /**
   * `all` while nothing was ever checked (or the user unchecked everything), `some` with the checked paths among
   * `allPaths` in that order, `gone` when there was a selection and a reload removed every track of it.
   */
  scope(allPaths: string[]): ToolScope
  /** Forgets every checked path not in `paths` (a reload no longer lists it); remembers when that empties a selection. */
  prune(paths: string[]): void
  /** Forgets that the selection vanished, so the next tool takes every track again. */
  clearGone(): void
  /** Whether the whole selection vanished and the user has not acted since. */
  readonly gone: boolean
}

export function createTrackSelection(): TrackSelection {
  let selected = $state.raw<ReadonlySet<string>>(new Set())
  let gone = $state(false)

  return {
    get selected() {
      return selected
    },
    get gone() {
      return gone
    },
    toggle(path) {
      const next = new Set(selected)
      if (!next.delete(path)) next.add(path)
      selected = next
      gone = false
    },
    setAll(paths, on) {
      const next = new Set(selected)
      for (const path of paths) {
        if (on) next.add(path)
        else next.delete(path)
      }
      selected = next
      gone = false
    },
    scope(allPaths) {
      const checked = allPaths.filter((path) => selected.has(path))
      if (checked.length > 0) return { kind: 'some', paths: checked }
      return gone ? { kind: 'gone' } : { kind: 'all' }
    },
    prune(paths) {
      const keep = new Set(paths)
      const next = new Set([...selected].filter((path) => keep.has(path)))
      if (next.size === selected.size) return
      if (next.size === 0) gone = true
      selected = next
    },
    clearGone() {
      gone = false
    },
  }
}
