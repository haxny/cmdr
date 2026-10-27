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

/** The outcome of `first`, plus "Write anyway"'s `second` report or the user's skip of the would-drop files. */
export function outcomeOf(
  first: TagApplyReport,
  after: { second?: TagApplyReport; skipped?: boolean } = {},
): ApplyOutcome {
  const { second, skipped = false } = after
  return {
    written: first.written.length + (second?.written.length ?? 0),
    failed: [...first.failed, ...(second?.failed ?? [])],
    changedOnDisk: [...first.changedOnDisk, ...(second?.changedOnDisk ?? [])],
    skipped: skipped ? first.wouldDrop : (second?.wouldDrop ?? []),
    undoUnavailable: first.undoUnavailable || second?.undoUnavailable === true,
  }
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
