/**
 * Pure readings of a tool line's typed facts (`ToolOutcome`, `ToolSubject`), shared by the live
 * stream and the history fold so both read a line the same way.
 */

import type { ToolOutcome, ToolSubject } from '#lib/ipc/bindings.js'

/** Which outcomes got what the call asked for. A `Record` over every kind, so a new outcome
 * variant is a compile error here rather than a silent guess. Mirrors `ToolOutcome::ok`. */
const OUTCOME_OK: Record<ToolOutcome['kind'], boolean> = {
  items: true,
  matches: true,
  nothingFound: true,
  answered: true,
  indexUnavailable: true,
  staged: true,
  notStaged: false,
  saved: true,
  refused: false,
}

/** Whether a call got what it asked for: a refusal and a plan that staged nothing didn't. */
export function outcomeOk(outcome: ToolOutcome): boolean {
  return OUTCOME_OK[outcome.kind]
}

/** The one path a line names today, for its "looked at X" suffix: a folder's, or the first
 * file's. `null` for every other subject. */
export function subjectPath(subject: ToolSubject): string | null {
  if (subject.kind === 'folder') return subject.path
  if (subject.kind === 'files') return subject.firstPath
  return null
}
