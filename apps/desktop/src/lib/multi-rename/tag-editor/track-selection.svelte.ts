/**
 * Which tracks of the Tags window's Tracks pane are selected, the way files are in a pane: the index set and the
 * keyboard/mouse fill rules are the file panes' own (`createSelectionState`), over the tracks in view order. A tool
 * (Case, Renumber, find/replace…) works on the selected tracks alone, and on every track while none is selected.
 * `sync` carries the selection across a reload by path, so a reordered or shrunk list keeps the right tracks.
 */

import { createSelectionState } from '#lib/file-explorer/pane/selection-state.svelte.js'

/* eslint-disable svelte/prefer-svelte-reactivity -- the Sets here are per-call snapshots (a read-only answer, a lookup), never state; the reactive set is `createSelectionState`'s */

/** What a tool works on: every track (none selected), the selected ones, or nothing because they all vanished. */
export type ToolScope = { kind: 'all' } | { kind: 'some'; paths: string[] } | { kind: 'gone' }

/** A keyboard Shift-move: the cursor left `from` for `to`; `overflow` when the move hit an end of the list. */
export interface ExtendMove {
  from: number
  to: number
  overflow: boolean
}

export interface TrackSelection {
  /** The selected tracks' paths. A new set on every read; never mutate it. */
  readonly selected: ReadonlySet<string>
  readonly count: number
  has(index: number): boolean
  /** The tracks in view order (absolute paths, as `TrackRow.path`); keeps the selected ones that are still listed. */
  sync(paths: readonly string[]): void
  toggleAt(index: number): void
  /** ⇧↑ / ⇧↓ / ⇧PgDn…: the file panes' toggle-and-fill. */
  extend(move: ExtendMove): void
  /** ⇧-click: selects from the cursor to `index`, file-pane style. */
  extendTo(index: number, cursor: number): void
  selectAll(): void
  deselectAll(): void
  invert(): void
  /** The header box: every track (`true`) or none. */
  setAll(on: boolean): void
  /**
   * `all` while nothing is selected, `some` with the selected paths in view order, `gone` when there was a selection
   * and a reload removed every track of it.
   */
  scope(): ToolScope
  /** Forgets that the selection vanished, so the next tool takes every track again. */
  clearGone(): void
  /** Whether the whole selection vanished and the user has not acted since. */
  readonly gone: boolean
}

export function createTrackSelection(): TrackSelection {
  let gone = $state(false)
  const marks = createSelectionState({
    onChanged: () => {
      gone = false
    },
  })
  let paths = $state.raw<readonly string[]>([])

  function selectedPaths(): string[] {
    return marks.getSelectedIndices().map((index) => paths[index])
  }

  return {
    get selected() {
      return new Set(selectedPaths())
    },
    get count() {
      return marks.selectedIndices.size
    },
    get gone() {
      return gone
    },
    has(index) {
      return marks.selectedIndices.has(index)
    },
    sync(next) {
      if (next.length === paths.length && next.every((path, index) => path === paths[index])) return
      const before = new Set(selectedPaths())
      const kept = next.flatMap((path, index) => (before.has(path) ? [index] : []))
      const wasGone = gone
      paths = next
      // The selection is replaced with the kept rows; its `onChanged` must not clear a vanish (noted now or before).
      marks.setSelectedIndices(kept)
      gone = kept.length === 0 && (before.size > 0 || wasGone)
    },
    toggleAt(index) {
      if (index < 0 || index >= paths.length) return
      marks.toggleAt(index, false)
    },
    extend({ from, to, overflow }) {
      marks.handleShiftKeyboardNavigation(from, to, overflow, false)
    },
    extendTo(index, cursor) {
      marks.handleShiftMouseNavigation(index, cursor, false)
    },
    selectAll() {
      marks.selectAll(false, paths.length)
    },
    deselectAll() {
      marks.deselectAll()
    },
    invert() {
      marks.invertSelection(false, paths.length)
    },
    setAll(on) {
      if (on) marks.selectAll(false, paths.length)
      else marks.deselectAll()
    },
    scope() {
      const chosen = selectedPaths()
      if (chosen.length > 0) return { kind: 'some', paths: chosen }
      return gone ? { kind: 'gone' } : { kind: 'all' }
    },
    clearGone() {
      gone = false
    },
  }
}
