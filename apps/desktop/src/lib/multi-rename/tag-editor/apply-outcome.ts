/**
 * What a tag write did, in the terms the Apply panel shows: merged across "Write anyway", whose request re-sends
 * only the files that would lose data. Pure.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ContentKind, FailedRow, TagError, WouldDropRow } from '#lib/ipc/bindings.js'
import type { ApplyRequest, FileChangeView, TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
import type { TagStateError } from './tag-editor-state.svelte'

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
  allowRemovals: readonly string[] = [],
): ApplyRequest {
  const dropping = new Set(wouldDrop.map((row) => row.path))
  const skip = [...new Set(allPaths)].filter((path) => !dropping.has(path))
  return { kind, allowFrameLoss: true, skip, allowRemovals: [...allowRemovals] }
}

/**
 * The files whose kind removals a change list showed (an MP3's conductor under Audiobook, or "couldn't check"): the
 * only ones a write from that list may take a field from (`ApplyRequest.allowRemovals`).
 */
export function removalsShown(changes: readonly FileChangeView[]): string[] {
  return changes.filter((change) => change.removes.length > 0 || change.removesUnknown).map((change) => change.path)
}

/**
 * "Write anyway" after `last` asked: every path the write knows (the view's tracks and the report's) is skipped but
 * the would-drop ones. Shared by the Write dialog and ⌘⏎'s write, so both write exactly the same.
 */
export function writeAnywayAfter(
  kind: ContentKind,
  last: TagApplyReport,
  view: TagEditorView | null,
  allowRemovals: readonly string[] = [],
): ApplyRequest {
  const known = [
    ...(view?.tracks.map((track) => track.path) ?? []),
    ...last.written,
    ...last.failed.map((row) => row.path),
    ...last.changedOnDisk,
    ...last.wouldDrop.map((row) => row.path),
    ...last.wouldRemove.map((row) => row.path),
  ]
  return writeAnywayRequest(kind, last.wouldDrop, known, allowRemovals)
}

/**
 * Why a write (or its change list) was refused, worded, when no earlier apply of the press wrote: `noneWritten` when
 * the refusal came before any file was touched; another (a task that died) may come after some were.
 */
export function refusalOf(why: TagStateError['kind']): { noneWritten: boolean; reason: MessageKey } {
  switch (why) {
    case 'undo':
    case 'noAppDataDir':
      return { noneWritten: true, reason: 'multiRename.tags.undoStoreUnavailable' }
    case 'readOnly':
      return { noneWritten: true, reason: 'multiRename.tags.readOnly' }
    case 'noSession':
    case 'notOpen':
      return { noneWritten: true, reason: 'multiRename.tags.write.sessionGone' }
    default:
      return { noneWritten: false, reason: 'multiRename.tags.write.stopped' }
  }
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
  badDate: 'multiRename.tags.write.error.badDate',
  wouldDrop: 'multiRename.tags.write.error.wouldDrop',
  unwritableExtra: 'multiRename.tags.write.error.unwritableExtra',
}

/** Why a file wasn't written, by the error's kind (its OS message stays out of the UI). */
export function failureKey(error: TagError): MessageKey {
  return FAILURE_KEYS[error.type]
}

/** What `failureKey`'s message names: a bad date shows the date, so the user knows what to correct. */
export function failureParams(error: TagError): Record<string, string> {
  return error.type === 'badDate' ? { date: error.data } : {}
}

export function fileNameOf(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1)
}
