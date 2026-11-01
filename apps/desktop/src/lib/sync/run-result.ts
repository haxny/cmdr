/**
 * The result screen's lines: how every row of a run ended, by its typed reason (`RunItemResult`), never by a
 * message. `held` in the report counts guard holds and removals held after a failed copy together; the items tell
 * them apart.
 */
import type { RunItemResult, RunReportWire } from '#lib/tauri-commands/sync.js'

export type ResultKind = RunItemResult | 'heldByGuard'

export interface ResultLine {
  kind: ResultKind
  count: number
}

/** The order the result lists them in: what went well, then what was left alone, then what never started. */
const ORDER: readonly ResultKind[] = [
  'done',
  'changedSinceCompare',
  'noTrash',
  'couldNotDo',
  'versionsSpotTaken',
  'versionsFolderUnsafe',
  'keptNotReplaced',
  'heldAfterFailedCopy',
  'heldByGuard',
  'notStarted',
]

export function resultLines(report: RunReportWire): ResultLine[] {
  const counts = new Map<ResultKind, number>()
  for (const { result } of report.items) counts.set(result, (counts.get(result) ?? 0) + 1)
  counts.set('done', report.done)
  counts.set('heldByGuard', Math.max(0, report.held - (counts.get('heldAfterFailedCopy') ?? 0)))
  return ORDER.map((kind) => ({ kind, count: counts.get(kind) ?? 0 })).filter((line) => line.count > 0)
}
