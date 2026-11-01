/**
 * The result screen's lines: how every row of a run ended, by its typed reason (`RunItemResult`), never by a
 * message. Rows a guard held never reached the run, so they come as their own list (`heldByGuard`, copies and
 * replacements too); removals held after a failed copy are items.
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
  counts.set('heldByGuard', report.heldByGuard.length)
  return ORDER.map((kind) => ({ kind, count: counts.get(kind) ?? 0 })).filter((line) => line.count > 0)
}

/** Plan indices of every row that didn't end done, guard holds included, for "Show in grid". */
export function notDoneItems(report: RunReportWire): number[] {
  const unfinished = report.items.filter((item) => item.result !== 'done').map((item) => item.item)
  return [...unfinished, ...report.heldByGuard.map((row) => row.item)]
}
