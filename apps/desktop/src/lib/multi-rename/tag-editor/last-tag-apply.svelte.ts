/**
 * The last tag write, which "Undo tag changes" reverses. One write is every apply of one "Write tags" press: the
 * first one and, after "Write anyway", the one for the files that would lose data, so Undo reverses them all.
 * Outlives the sheet until the next write that writes something, as `last-run.svelte.ts` does for Rename. Not
 * persisted.
 */

/** One apply Undo can reverse, and how many files it wrote. */
export interface UndoableApply {
  applyId: string
  written: number
}

export interface LastTagApply {
  /** In the order they were written. */
  applies: readonly UndoableApply[]
  /** Some files of this write were written without their old tags kept: Undo can't reach them. */
  partial: boolean
}

let last = $state.raw<LastTagApply | null>(null)

export function getLastTagApply(): LastTagApply | null {
  return last
}

export function setLastTagApply(next: LastTagApply | null): void {
  last = next
}

/** How many files Undo would put back. */
export function undoableCount(write: LastTagApply): number {
  return write.applies.reduce((sum, apply) => sum + apply.written, 0)
}
