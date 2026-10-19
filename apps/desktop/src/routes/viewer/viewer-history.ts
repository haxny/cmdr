/**
 * A viewer window's document history: the files its Markdown links led to, stepped through
 * with Back / Forward like a browser. Pure state, no IPC: the page opens the target and
 * only then calls `step` or `visit`, so an open that doesn't happen leaves the history
 * where it was.
 */

/** How many documents a window remembers; the oldest go first. */
export const MAX_HISTORY_ENTRIES = 100

export interface HistoryEntry {
  /** The document's path on the window's volume, as `viewerOpen` takes it. */
  path: string
  /** Where the rendered view was scrolled when the document was left. */
  scrollTop: number
}

export type HistoryDirection = 'back' | 'forward'

export interface ViewerHistory {
  readonly current: HistoryEntry
  readonly canGoBack: boolean
  readonly canGoForward: boolean
  /** The entry a step that way would show, or `null`. Doesn't move. */
  target(direction: HistoryDirection): HistoryEntry | null
  /** Moves one entry that way, once its document is showing. A no-op at either end. */
  step(direction: HistoryDirection): void
  /** A followed link opened `path`: it becomes current, and the forward entries go. */
  visit(path: string): void
  /** Records the current document's scroll position, before it's left. */
  rememberScroll(scrollTop: number): void
}

/** Starts a history whose only entry is the document the window opened with. */
export function createViewerHistory(path: string): ViewerHistory {
  const entries: HistoryEntry[] = [{ path, scrollTop: 0 }]
  let index = 0

  /** The index a step that way lands on, or `null` past either end. */
  const indexToward = (direction: HistoryDirection): number | null => {
    const next = index + (direction === 'back' ? -1 : 1)
    return next >= 0 && next < entries.length ? next : null
  }

  return {
    get current() {
      return entries[index]
    },
    get canGoBack() {
      return index > 0
    },
    get canGoForward() {
      return index < entries.length - 1
    },
    target(direction) {
      const next = indexToward(direction)
      return next === null ? null : { ...entries[next] }
    },
    step(direction) {
      index = indexToward(direction) ?? index
    },
    visit(nextPath) {
      entries.splice(index + 1, entries.length, { path: nextPath, scrollTop: 0 })
      if (entries.length > MAX_HISTORY_ENTRIES) entries.splice(0, entries.length - MAX_HISTORY_ENTRIES)
      index = entries.length - 1
    },
    rememberScroll(scrollTop) {
      entries[index].scrollTop = scrollTop
    },
  }
}
