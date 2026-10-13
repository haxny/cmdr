/**
 * One view's state over a transfer tree: its hand toggles, whether it follows
 * the work, where it's scrolled, the last window the backend answered, and the
 * request coalescer that keeps at most one request in flight.
 *
 * ❌ No tree model here: the backend owns it and answers windows. This holds
 * only what the backend can't know (what THIS view toggled and scrolled to).
 */

import type {
  TransferTreeAncestor,
  TransferTreeRow,
  TransferTreeView,
  TransferTreeViewRequest,
} from '#lib/tauri-commands/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { createCoalesced, createThrottle } from '#lib/utils/timing.js'
import type { TransferTreeSource } from './transfer-tree-source'
import { MAX_PINNED } from './pinned-headers'

const log = getAppLogger('transferTree')

/**
 * Rows kept above the work while following, so the folder it sits in stays in sight. A
 * deeper row gets one per pinned header (up to `MAX_PINNED`), so the stack never hides it.
 */
export const FOLLOW_LEAD = 2

/** Rows fetched above and below the viewport, so a short scroll lands on loaded rows. */
export const OVERSCAN_ROWS = 10

/** The most hand toggles of each kind a request carries: the backend's own cap. */
export const MAX_TOGGLES = 2_000

/** The most rows one response carries: the backend's own cap. */
const MAX_LIMIT = 200

/**
 * The least time between two tick-driven pulls: at most five a second per view. The
 * local engine ticks once per file, so a run of tiny files would otherwise pull back
 * to back. A clock, ❌ not an animation frame: the E2E app gets no frames.
 */
export const TICK_PULL_INTERVAL_MS = 200

/** Why the view stopped asking: the operation ended, or it never builds a tree. */
export type TransferTreeEnded = 'finished' | 'notATransfer'

export interface TransferTreeViewState {
  /** The last window the backend answered, `null` before the first. */
  readonly view: TransferTreeView | null
  /** Set once the backend says there's nothing more to ask for. */
  readonly ended: TransferTreeEnded | null
  readonly following: boolean
  /** Where the viewport's top row belongs while following, or `null` when it doesn't move. */
  readonly followTop: number | null
  readonly expanded: readonly number[]
  readonly collapsed: readonly number[]
  /** The operation advanced: pull a fresh window, at most every {@link TICK_PULL_INTERVAL_MS}. */
  refresh: () => void
  setViewportRows: (rows: number) => void
  /**
   * The viewport's top row moved, by anyone. ❌ A scroll alone never pauses following:
   * the view's own follow scrolls, and the browser clamping a list that shrank, fire the
   * same event a user's does. The user's INTENT pauses it ({@link pauseFollowing}).
   */
  scrolledTo: (topRow: number) => void
  /** The user scrolled (wheel, scrollbar) or moved by keyboard: stay where they are. */
  pauseFollowing: () => void
  resumeFollowing: () => void
  /** The user expanded (`expandedNow: false`) or collapsed (`true`) a row. */
  toggle: (id: number, expandedNow: boolean) => void
  /** The row at a flat index, if the last window holds it. */
  rowAt: (index: number) => TransferTreeRow | undefined
  /** A loaded row's open ancestor rows, outermost first: earlier rows of the window, else the view's `ancestors`. */
  ancestorsOf: (index: number) => TransferTreeAncestor[]
  /** The flat index of a row's parent row, `null` for a top-level row or one the window doesn't know. */
  parentIndexOf: (index: number) => number | null
  /** The view went away: answer nothing more, ask nothing more. */
  dispose: () => void
}

export interface TransferTreeViewOptions {
  /** Runs scroll-driven work once per frame. Defaults to `requestAnimationFrame`. */
  schedule?: (run: () => void) => void
  /** The least time between two tick-driven pulls. Defaults to {@link TICK_PULL_INTERVAL_MS}. */
  tickPullIntervalMs?: number
}

function defaultSchedule(run: () => void): void {
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run)
  else setTimeout(run, 0)
}

/** `id` moved to the newest end of `ids`, the oldest dropped past the cap. */
function withNewest(ids: readonly number[], id: number): number[] {
  const next = ids.filter((existing) => existing !== id)
  next.push(id)
  return next.length > MAX_TOGGLES ? next.slice(next.length - MAX_TOGGLES) : next
}

export function createTransferTreeViewState(
  source: TransferTreeSource,
  { schedule = defaultSchedule, tickPullIntervalMs = TICK_PULL_INTERVAL_MS }: TransferTreeViewOptions = {},
): TransferTreeViewState {
  let view = $state.raw<TransferTreeView | null>(null)
  let ended = $state<TransferTreeEnded | null>(null)
  let following = $state(true)
  let expanded = $state.raw<number[]>([])
  let collapsed = $state.raw<number[]>([])
  let viewportRows = $state(8)
  let topRow = 0

  let framePending = false
  let disposed = false

  function isDisposed(): boolean {
    return disposed
  }

  function request(): TransferTreeViewRequest {
    return {
      expanded,
      collapsed,
      // The window reaches as far above the work as below the viewport, so the rows the
      // pinned headers cover and a short scroll up land on loaded rows.
      window: following
        ? { kind: 'follow', lead: OVERSCAN_ROWS }
        : { kind: 'at', offset: Math.max(0, topRow - OVERSCAN_ROWS) },
      limit: Math.min(MAX_LIMIT, viewportRows + 2 * OVERSCAN_ROWS),
    }
  }

  // One request in flight; whatever asks meanwhile queues ONE more. The request is
  // built when it's SENT, so everything that changed meanwhile rides along.
  const pulls = createCoalesced<null>(async () => {
    if (ended !== null || disposed) return
    try {
      const answer = await source.view(request())
      // Read through a call: the view can go away while the answer is on its way, which
      // straight-line narrowing from the check above can't see.
      if (isDisposed()) return
      if ('refusal' in answer) {
        ended = answer.refusal.type === 'unknown_operation' ? 'finished' : 'notATransfer'
        return
      }
      view = answer
    } catch (error: unknown) {
      log.warn('Transfer tree view failed: {error}', { error: String(error) })
    }
  })

  function pull(): void {
    void pulls.call(null)
  }

  // Ticks pull at once, then at most once per interval, and the last tick of a burst
  // always gets its pull (`createThrottle`'s trailing call), so the final state lands.
  const tickPulls = createThrottle(pull, tickPullIntervalMs)

  /** The top row that puts the work `FOLLOW_LEAD` rows down (more under a deep stack), clamped so the viewport stays full. */
  function followPosition(): number | null {
    if (view === null || !view.ready || view.focusRow === null) return null
    const depth = view.rows[view.focusRow - view.offset]?.depth ?? 0
    const lead = Math.max(FOLLOW_LEAD, Math.min(MAX_PINNED, depth))
    const lastTop = Math.max(0, view.totalRows - viewportRows)
    return Math.min(lastTop, Math.max(0, view.focusRow - lead))
  }

  function ancestorsOf(index: number): TransferTreeAncestor[] {
    if (view === null) return []
    const row = view.rows[index - view.offset] as TransferTreeRow | undefined
    if (row === undefined) return []
    const out: TransferTreeAncestor[] = []
    let depth = row.depth - 1
    for (let at = index - 1; at >= view.offset && depth >= 0; at--) {
      const above = view.rows[at - view.offset]
      if (above.depth === depth) {
        out.push({ index: at, row: above })
        depth--
      }
    }
    // Whatever the window's rows didn't reach sits above its first row.
    for (; depth >= 0; depth--) {
      const above = view.ancestors.at(depth)
      if (above) out.push(above)
    }
    return out.reverse()
  }

  /** At most one pull per frame for scrolls, which fire far faster than that. */
  function pullNextFrame(): void {
    if (framePending) return
    framePending = true
    schedule(() => {
      framePending = false
      pull()
    })
  }

  return {
    get view() {
      return view
    },
    get ended() {
      return ended
    },
    get following() {
      return following
    },
    get followTop() {
      return following ? followPosition() : null
    },
    get expanded() {
      return expanded
    },
    get collapsed() {
      return collapsed
    },
    refresh: () => {
      tickPulls.call()
    },
    setViewportRows(rows) {
      if (rows === viewportRows || rows <= 0) return
      viewportRows = rows
      pullNextFrame()
    },
    scrolledTo(row) {
      if (row === topRow) return
      topRow = row
      if (!following) pullNextFrame()
    },
    pauseFollowing() {
      if (!following) return
      // The viewport sits where following put it, whether or not a scroll event said so.
      topRow = followPosition() ?? topRow
      following = false
    },
    resumeFollowing() {
      following = true
      pull()
    },
    toggle(id, expandedNow) {
      if (expandedNow) {
        expanded = expanded.filter((existing) => existing !== id)
        collapsed = withNewest(collapsed, id)
      } else {
        collapsed = collapsed.filter((existing) => existing !== id)
        expanded = withNewest(expanded, id)
      }
      pull()
    },
    rowAt(index) {
      if (view === null) return undefined
      return view.rows[index - view.offset]
    },
    dispose() {
      disposed = true
      tickPulls.cancel()
      pulls.cancel()
    },
    ancestorsOf,
    parentIndexOf(index) {
      return ancestorsOf(index).at(-1)?.index ?? null
    },
  }
}
