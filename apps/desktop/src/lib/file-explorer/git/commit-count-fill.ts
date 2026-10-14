/**
 * Asks for the files-changed counts of the `.git/commits/` rows on screen that
 * the walk left pending (`gitMeta.kind === 'countPending'`, past the newest
 * 5,000). The backend patches the counted rows into the listing, so they arrive
 * as an ordinary `directory-diff`: the cells fill, a Size sort places them, and
 * scrolling back keeps them. Fire-and-forget, the folder-size pattern.
 *
 * Each row is asked for once per listing: a re-fetch of the same window (every
 * diff causes one) must not re-queue the same diffs.
 */

import { fillGitCommitCounts } from '#lib/tauri-commands/index.js'
import type { FileEntry } from '../types'

/** Paths already asked for, per listing. A new listing starts empty. */
const asked = new Map<string, Set<string>>()

/** Listings remembered at once; both panes plus a recent one is plenty. */
const MAX_LISTINGS = 4

function askedFor(listingId: string): Set<string> {
  let paths = asked.get(listingId)
  if (!paths) {
    paths = new Set<string>()
    asked.set(listingId, paths)
    if (asked.size > MAX_LISTINGS) {
      const oldest = asked.keys().next().value
      if (oldest !== undefined) asked.delete(oldest)
    }
  }
  return paths
}

export function requestPendingCommitCounts(listingId: string, entries: readonly FileEntry[]): void {
  const done = askedFor(listingId)
  const paths = entries
    .filter((entry) => entry.gitMeta?.kind === 'countPending' && !done.has(entry.path))
    .map((entry) => entry.path)
  if (paths.length === 0) return
  for (const path of paths) done.add(path)
  void fillGitCommitCounts(listingId, paths)
}

/** Forgets every listing, so each test starts clean. */
export function _resetCommitCountFillForTests(): void {
  asked.clear()
}
