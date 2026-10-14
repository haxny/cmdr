/**
 * Getting the files-changed counts the `.git/commits/` walk leaves out
 * (`gitMeta.kind === 'countPending'`, shown as `…`).
 *
 * The first time a listing shows a pending row, this asks the backend for a
 * whole-listing pass (`countAllGitCommits`). A listing of at most 5,000 commits is
 * counted in full in the background, which is what makes Size sort work there. A
 * longer one answers `tooMany`, and from then on only the rows on screen are
 * counted, each once. The backend patches counted rows into the listing, so they
 * arrive as an ordinary `directory-diff`: the cells fill, a Size sort places them,
 * and scrolling back keeps them.
 *
 * The `tooMany` answer is also what `commitSizeSortLimit` reports, so the pane can
 * refuse Size sort with a reason (`pane/sort-operations.ts`).
 */

import { countAllGitCommits, fillGitCommitCounts } from '#lib/tauri-commands/index.js'
import type { FileEntry } from '../types'

/** Where a listing's counting stands. */
type ListingCount = { kind: 'asking' } | { kind: 'counting' } | { kind: 'tooMany'; limit: number; asked: Set<string> }

const listings = new Map<string, ListingCount>()

/** Listings remembered at once; both panes plus a few recent ones is plenty. */
const MAX_LISTINGS = 8

function remember(listingId: string, state: ListingCount): void {
  listings.set(listingId, state)
  if (listings.size > MAX_LISTINGS) {
    const oldest = listings.keys().next().value
    if (oldest !== undefined) listings.delete(oldest)
  }
}

function pendingPaths(entries: readonly FileEntry[]): string[] {
  return entries.filter((entry) => entry.gitMeta?.kind === 'countPending').map((entry) => entry.path)
}

/** Counts the pending rows not asked for yet, on a listing too long for the full pass. */
function fillOnScreen(listingId: string, state: { asked: Set<string> }, paths: string[]): void {
  const fresh = paths.filter((path) => !state.asked.has(path))
  if (fresh.length === 0) return
  for (const path of fresh) state.asked.add(path)
  void fillGitCommitCounts(listingId, fresh)
}

export function requestPendingCommitCounts(listingId: string, entries: readonly FileEntry[]): void {
  const paths = pendingPaths(entries)
  if (paths.length === 0) return
  const state = listings.get(listingId)
  if (state?.kind === 'tooMany') {
    fillOnScreen(listingId, state, paths)
    return
  }
  // Already asked, and either still waiting or the full pass covers these rows.
  if (state) return

  remember(listingId, { kind: 'asking' })
  void countAllGitCommits(listingId).then((pass) => {
    if (pass.type === 'tooMany') {
      const tooMany = { kind: 'tooMany' as const, limit: pass.limit, asked: new Set<string>() }
      remember(listingId, tooMany)
      // The rows that triggered the ask are still on screen; count them now.
      fillOnScreen(listingId, tooMany, paths)
    } else {
      remember(listingId, { kind: 'counting' })
    }
  })
}

/**
 * The commit count past which `listingId` can't sort by Size, or `null` when it
 * can (or isn't a commit list at all).
 */
export function commitSizeSortLimit(listingId: string): number | null {
  const state = listings.get(listingId)
  return state?.kind === 'tooMany' ? state.limit : null
}

/** Forgets every listing, so each test starts clean. */
export function _resetCommitCountFillForTests(): void {
  listings.clear()
}
