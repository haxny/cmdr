/**
 * The last tag write, which "Undo tag changes" reverses. One write is every apply of one "Write tags" press: the
 * first one and, after "Write anyway", the one for the files that would lose data, so Undo reverses them all.
 * Outlives the sheet until the next write, as `last-run.svelte.ts` does for Rename. Not persisted.
 */

export interface LastTagApply {
  /** The backend's apply ids, in the order they were written. */
  applyIds: readonly string[]
  /** How many files they wrote. */
  written: number
}

let last = $state.raw<LastTagApply | null>(null)

export function getLastTagApply(): LastTagApply | null {
  return last
}

export function setLastTagApply(next: LastTagApply | null): void {
  last = next
}
