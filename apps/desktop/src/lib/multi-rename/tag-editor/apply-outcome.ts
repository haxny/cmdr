/**
 * What a tag write did, in the terms the Apply panel shows: merged across "Write anyway", whose request re-sends
 * only the files that would lose data. Pure.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ContentKind, FailedRow, TagError, WouldDropRow } from '#lib/ipc/bindings.js'
import type { ApplyRequest, TagApplyReport } from '#lib/tauri-commands/index.js'

export interface ApplyOutcome {
  written: number
  /** Not written, with why. */
  failed: FailedRow[]
  /** Not written: the file changed on disk since it was read. */
  changedOnDisk: string[]
  /** Not written: the user chose "Skip these files". */
  skipped: WouldDropRow[]
  /** Some files were written without their old tags kept: that part can't be undone. */
  undoUnavailable: boolean
}

/** "Write anyway": frame loss allowed for the would-drop files, every other path skipped so nothing else is written. */
export function writeAnywayRequest(
  kind: ContentKind,
  wouldDrop: readonly WouldDropRow[],
  allPaths: readonly string[],
): ApplyRequest {
  const dropping = new Set(wouldDrop.map((row) => row.path))
  const skip = [...new Set(allPaths)].filter((path) => !dropping.has(path))
  return { kind, allowFrameLoss: true, skip }
}

type FileStatus =
  | { kind: 'written' }
  | { kind: 'failed'; row: FailedRow }
  | { kind: 'changedOnDisk' }
  | { kind: 'wouldDrop'; row: WouldDropRow }

/** Each path's status in one report. */
function statusesOf(report: TagApplyReport): [string, FileStatus][] {
  return [
    ...report.written.map((path): [string, FileStatus] => [path, { kind: 'written' }]),
    ...report.failed.map((row): [string, FileStatus] => [row.path, { kind: 'failed', row }]),
    ...report.changedOnDisk.map((path): [string, FileStatus] => [path, { kind: 'changedOnDisk' }]),
    ...report.wouldDrop.map((row): [string, FileStatus] => [row.path, { kind: 'wouldDrop', row }]),
  ]
}

/**
 * The outcome of one "Write tags" press: every apply it made, in order (the first, then "Write anyway" or a write
 * after the changes were listed again). Each file counts once, by its latest status; one still left as would-drop
 * was skipped (by the user, or because "Write anyway" was refused).
 */
export function outcomeOf(reports: readonly TagApplyReport[]): ApplyOutcome {
  const latest = new Map<string, FileStatus>(reports.flatMap(statusesOf))
  const outcome: ApplyOutcome = {
    written: 0,
    failed: [],
    changedOnDisk: [],
    skipped: [],
    undoUnavailable: reports.some((report) => report.undoUnavailable),
  }
  for (const [path, status] of latest) {
    if (status.kind === 'written') outcome.written += 1
    else if (status.kind === 'failed') outcome.failed.push(status.row)
    else if (status.kind === 'changedOnDisk') outcome.changedOnDisk.push(path)
    else outcome.skipped.push(status.row)
  }
  return outcome
}

const FAILURE_KEYS: Readonly<Record<TagError['type'], MessageKey>> = {
  notSupported: 'multiRename.tags.write.error.notSupported',
  io: 'multiRename.tags.write.error.io',
  parse: 'multiRename.tags.write.error.parse',
  write: 'multiRename.tags.write.error.write',
  wouldDrop: 'multiRename.tags.write.error.wouldDrop',
  unwritableExtra: 'multiRename.tags.write.error.unwritableExtra',
}

/** Why a file wasn't written, by the error's kind (its OS message stays out of the UI). */
export function failureKey(error: TagError): MessageKey {
  return FAILURE_KEYS[error.type]
}

export function fileNameOf(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1)
}
