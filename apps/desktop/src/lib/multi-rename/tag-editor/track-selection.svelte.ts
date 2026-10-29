/**
 * Which tracks of the Tags window's Tracks pane are selected, the way files are in a pane: the index set and the
 * keyboard/mouse fill rules are the file panes' own (`createSelectionState`), over the tracks in view order. A tool
 * (Case, Renumber, find/replace…) works on the selected tracks alone, and on every track while none is selected.
 * `sync` carries the selection across a reload by path, so a reordered or shrunk list keeps the right tracks.
 *
 * Under the Tracks pane's filter (`setShown`) the selection keeps its hidden tracks (`hiddenCount`); ⌘A selects
 * exactly the shown ones, invert and the header box act on the shown ones, and ⇧ fills skip the hidden rows. With
 * nothing selected the scope is the shown tracks, by path (`shown`), so a tool never reaches a hidden track.
 */

import { createSelectionState } from '#lib/file-explorer/pane/selection-state.svelte.js'

/* eslint-disable svelte/prefer-svelte-reactivity -- the Sets here are per-call snapshots (a read-only answer, a lookup), never state; the reactive set is `createSelectionState`'s */

/** What a tool works on: every track (none selected), the selected ones, or nothing because they all vanished. */
export type ToolScope =
  | { kind: 'all' }
  | { kind: 'some'; paths: string[] }
  /** Nothing selected and the filter on: the tracks it shows (none when it matches nothing). */
  | { kind: 'shown'; paths: string[] }
  | { kind: 'gone' }

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
  /** Selected tracks the filter hides. */
  readonly hiddenCount: number
  /** The paths the scope names: the selected tracks, else the shown ones under a filter, else none (every track). */
  readonly scoped: ReadonlySet<string>
  has(index: number): boolean
  /** The tracks the filter shows (paths), or `null` with no filter. */
  setShown(paths: readonly string[] | null): void
  /** The tracks in view order (absolute paths, as `TrackRow.path`); keeps the selected ones that are still listed. */
  sync(paths: readonly string[]): void
  toggleAt(index: number): void
  /** ⇧↑ / ⇧↓ / ⇧PgDn…: the file panes' toggle-and-fill. */
  extend(move: ExtendMove): void
  /** ⇧-click: selects from the cursor to `index`, file-pane style. */
  extendTo(index: number, cursor: number): void
  /** ⌘A: every track, or exactly the shown ones under a filter. */
  selectAll(): void
  /** ⌘⇧A: none, hidden ones too. */
  deselectAll(): void
  invert(): void
  /** The header box: every shown track (`true`) or none of them. */
  setAll(on: boolean): void
  /**
   * `all` while nothing is selected, `some` with the selected paths in view order, `shown` with the filter's tracks
   * while it is on and nothing is selected, `gone` when there was a selection and a reload removed every track of it.
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
  let shownPaths = $state.raw<ReadonlySet<string> | null>(null)
  /** The shown rows' indices in view order, or `null` with no filter. */
  const shown = $derived(
    shownPaths === null ? null : paths.flatMap((path, index) => (shownPaths?.has(path) === true ? [index] : [])),
  )

  function selectedPaths(): string[] {
    return marks.getSelectedIndices().map((index) => paths[index])
  }

  /** The shown rows from `a` to `b` (both ends, any order), as indices. */
  function shownBetween(a: number, b: number): number[] {
    const low = Math.min(a, b)
    const high = Math.max(a, b)
    return (shown ?? []).filter((index) => index >= low && index <= high)
  }

  /** ⇧↓ over the shown rows: the file panes' toggle-and-fill (`createSelectionState`), stepping over the hidden ones. */
  function extendShown(rows: readonly number[], { from, to, overflow }: ExtendMove): void {
    const fromAt = rows.indexOf(from)
    const toAt = rows.indexOf(to)
    if (fromAt < 0 || toAt < 0) return
    const step = Math.sign(toAt - fromAt)
    const endFill = overflow ? toAt : toAt - step
    const filled = [from]
    for (let at = fromAt + step; step !== 0 && (step > 0 ? at <= endFill : at >= endFill); at += step)
      filled.push(rows[at])
    marks.applyIndices(filled, marks.selectedIndices.has(from) ? 'remove' : 'add', false)
  }

  function selectAll(): void {
    if (shown === null) marks.selectAll(false, paths.length)
    else marks.setSelectedIndices(shown)
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
    get hiddenCount() {
      if (shown === null) return 0
      const visible = new Set(shown)
      return marks.getSelectedIndices().filter((index) => !visible.has(index)).length
    },
    get scoped() {
      if (marks.selectedIndices.size > 0) return new Set(selectedPaths())
      return new Set(shown?.map((index) => paths[index]) ?? [])
    },
    setShown(next) {
      shownPaths = next === null ? null : new Set(next)
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
    extend(move) {
      if (shown === null) marks.handleShiftKeyboardNavigation(move.from, move.to, move.overflow, false)
      else extendShown(shown, move)
    },
    extendTo(index, cursor) {
      if (shown === null) marks.handleShiftMouseNavigation(index, cursor, false)
      else marks.applyIndices(shownBetween(cursor, index), 'add', false)
    },
    selectAll,
    deselectAll() {
      marks.deselectAll()
    },
    invert() {
      if (shown === null) {
        marks.invertSelection(false, paths.length)
        return
      }
      const visible = new Set(shown)
      const hidden = marks.getSelectedIndices().filter((index) => !visible.has(index))
      marks.setSelectedIndices([...hidden, ...shown.filter((index) => !marks.selectedIndices.has(index))])
    },
    setAll(on) {
      if (on) selectAll()
      else if (shown === null) marks.deselectAll()
      else marks.applyIndices(shown, 'remove', false)
    },
    scope() {
      const chosen = selectedPaths()
      if (chosen.length > 0) return { kind: 'some', paths: chosen }
      if (gone) return { kind: 'gone' }
      return shown === null ? { kind: 'all' } : { kind: 'shown', paths: shown.map((index) => paths[index]) }
    },
    clearGone() {
      gone = false
    },
  }
}
