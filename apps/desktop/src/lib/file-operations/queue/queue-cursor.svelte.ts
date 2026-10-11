/**
 * The operation queue window's row cursor, and the "Cancel selected" set the cursor keys
 * extend. Pure bookkeeping over operation ids: the page owns the keys, the focus, and the
 * DOM (`routes/queue/+page.svelte`); DETAILS § "Keyboard".
 *
 * The cursor is an operation ID, never an index: `operations-changed` rebuilds the rows
 * whenever anything moves, and a reorder or a finished operation would otherwise slide a
 * different operation under it.
 */

import type { SvelteSet } from 'svelte/reactivity'

export type CursorStep = 'previous' | 'next' | 'first' | 'last'

export interface QueueCursorDeps {
  /** The rows' operation ids, top to bottom, read live. */
  ids: () => readonly string[]
  /** The "Cancel selected" set, owned by the page. */
  selection: SvelteSet<string>
  /** Whether a row can be in that set: a settled failure has nothing left to cancel. */
  isSelectable: (id: string) => boolean
}

/** Where a ⇧-arrow range started, and what was selected before it began. */
interface Anchor {
  id: string
  base: readonly string[]
}

export interface QueueCursor {
  readonly cursorId: string | null
  /** Puts the cursor on `id` (a click, or focus landing on a row). Ends any ⇧-arrow range. */
  place: (id: string) => void
  /** Moves the cursor and returns where it landed. Ends any ⇧-arrow range. */
  move: (to: CursorStep) => string | null
  /** Moves the cursor one row and makes the selection the range from the anchor to it,
   *  plus whatever was selected before the range began. */
  extend: (to: 'previous' | 'next') => string | null
  /** Space: checks or unchecks the row under the cursor. */
  toggle: () => void
  /** ⌘A: checks every row that can still be canceled. */
  selectAll: () => void
  /** Follows the rows after they change: the cursor keeps its operation, or takes the row
   *  that slid into its place when that operation left. */
  reconcile: () => void
}

/** Where `to` lands from `cursorId`. No cursor yet: ↓ starts at the top, ↑ at the bottom. */
function step(ids: readonly string[], cursorId: string | null, to: CursorStep): string | null {
  if (ids.length === 0) return null
  const last = ids.length - 1
  const at = cursorId === null ? -1 : ids.indexOf(cursorId)
  switch (to) {
    case 'first':
      return ids[0]
    case 'last':
      return ids[last]
    case 'next':
      return ids[at === -1 ? 0 : Math.min(at + 1, last)]
    case 'previous':
      return ids[at === -1 ? last : Math.max(at - 1, 0)]
  }
}

export function createQueueCursor({ ids, selection, isSelectable }: QueueCursorDeps): QueueCursor {
  let cursorId = $state<string | null>(null)
  let anchor: Anchor | null = null
  /** The ids as of the last change we saw, so a vanished cursor knows where it stood. */
  let knownIds: readonly string[] = ids()

  function startRangeAt(id: string): Anchor {
    anchor = { id, base: [...selection] }
    return anchor
  }

  return {
    get cursorId() {
      return cursorId
    },

    place(id) {
      cursorId = id
      anchor = null
    },

    move(to) {
      cursorId = step(ids(), cursorId, to)
      anchor = null
      return cursorId
    },

    extend(to) {
      const current = ids()
      const landing = step(current, cursorId, to)
      if (landing === null) return null
      const from = anchor !== null && current.includes(anchor.id) ? anchor : startRangeAt(cursorId ?? landing)
      cursorId = landing
      const [lo, hi] = [current.indexOf(from.id), current.indexOf(landing)].sort((a, b) => a - b)
      const range = current.slice(lo, hi + 1)
      const next = [...from.base, ...range].filter(isSelectable)
      for (const id of [...selection]) {
        if (!next.includes(id)) selection.delete(id)
      }
      for (const id of next) selection.add(id)
      return cursorId
    },

    toggle() {
      if (cursorId === null || !isSelectable(cursorId)) return
      if (selection.has(cursorId)) selection.delete(cursorId)
      else selection.add(cursorId)
      anchor = null
    },

    selectAll() {
      for (const id of ids()) {
        if (isSelectable(id)) selection.add(id)
      }
      anchor = null
    },

    reconcile() {
      const current = ids()
      if (cursorId !== null && !current.includes(cursorId)) {
        const stood = knownIds.indexOf(cursorId)
        cursorId = current.length === 0 ? null : current[Math.min(Math.max(stood, 0), current.length - 1)]
      }
      knownIds = current
    },
  }
}
