/**
 * What a transfer tree row says, as pure functions of the row: how full its
 * bar is, its count line, its accessible name, and its full relative path.
 * `TransferTreeRow.svelte` renders these; it decides nothing itself.
 */

import type { ProgressAxis, TransferTreeAncestor, TransferTreeRow, TreeRowState } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { formatUnitList } from '#lib/intl/list-format.js'
import { sizeDisplayParts } from '#lib/ui/size-display.js'
import { getUserFriendlyMessage } from '../transfer/transfer-error-messages'

/** The operations that build a transfer tree. Words the active state. */
export type TransferTreeOperation = 'copy' | 'move' | 'compress'

/**
 * One piece of a row's count line, joined by middle dots on screen. Sizes are plain
 * text: the tier colors `<Size>` paints suit one readout, and turned a tree of them loud.
 */
export interface CountSegment {
  text: string
  tone: 'quiet' | 'error'
}

/**
 * How full a row's bar is, 0 to 1. A folder fills along the operation's
 * bottleneck `axis`, the same on every row, so the bars agree with the ETA. A
 * file is bytes only. An empty total reads as full once the row has settled.
 */
export function barFraction(row: TransferTreeRow, axis: ProgressAxis): number {
  const byFiles = row.kind === 'folder' && axis === 'files'
  const done = byFiles ? row.filesDone : row.bytesDone
  const total = byFiles ? row.filesTotal : row.bytesTotal
  if (total <= 0) return row.state === 'done' ? 1 : 0
  return Math.min(1, done / total)
}

/** Whether a folder's counts read as progress ("312 / 1,000") rather than totals. */
function showsProgress(row: TransferTreeRow): boolean {
  return row.state === 'active' || row.state === 'failed'
}

/** The count line: "312 / 1,000 files · 1.2 / 4.0 GB", a file's size, or its outcome. */
export function countSegments(row: TransferTreeRow, operation: TransferTreeOperation): CountSegment[] {
  if (row.kind === 'file') return fileSegments(row, operation)
  const inProgress = showsProgress(row)
  const segments: CountSegment[] = [
    {
      text: inProgress
        ? tString('fileOperations.transferTree.filesProgress', {
            doneText: formatInteger(row.filesDone),
            totalText: formatInteger(row.filesTotal),
            total: row.filesTotal,
          })
        : filesTotalText(row.filesTotal),
      tone: 'quiet',
    },
  ]
  if (row.bytesTotal > 0) {
    segments.push({ text: sizeLine(inProgress ? row.bytesDone : null, row.bytesTotal), tone: 'quiet' })
  }
  if (row.skipped > 0) segments.push({ text: skippedText(row.skipped), tone: 'quiet' })
  if (row.failed > 0) segments.push({ text: failedText(row.failed), tone: 'error' })
  return segments
}

function fileSegments(row: TransferTreeRow, operation: TransferTreeOperation): CountSegment[] {
  switch (row.state) {
    case 'active':
      return [{ text: sizeLine(row.bytesDone, row.bytesTotal), tone: 'quiet' }]
    case 'skipped':
      return [{ text: stateWord('skipped', 'copy'), tone: 'quiet' }]
    case 'failed':
      return [{ text: failureReason(row, operation), tone: 'error' }]
    case 'waiting':
    case 'done':
      return [{ text: sizeLine(null, row.bytesTotal), tone: 'quiet' }]
  }
}

/**
 * Why a file failed, in the typed error pipeline's own words (its title), ❌
 * never new prose. A failed row without a typed error (none today) says only
 * that it failed.
 */
function failureReason(row: TransferTreeRow, operation: TransferTreeOperation): string {
  if (row.failure === null) return stateWord('failed', 'copy')
  return getUserFriendlyMessage(row.failure, operation).title
}

/**
 * The row's accessible name: what it shows, in the order it shows it, then its
 * state: "Photos 2024, 312 / 1,000 files, 1.2 GB / 4.0 GB, Copying". Each item
 * is a phrase of its own, listed the UI language's way, so no catalog string is
 * a sentence fragment.
 *
 * ❗ The visible text comes first and verbatim (WCAG 2.5.3, label in name):
 * speech control finds a row by the words on screen, so "312 of 1,000" for a
 * visible "312 / 1,000" would miss it. The state is last, and only when no
 * visible segment already says it (a skipped file shows "Skipped").
 */
export function accessibleName(row: TransferTreeRow, operation: TransferTreeOperation): string {
  const visible = countSegments(row, operation).map((segment) => segment.text)
  const state = stateWord(row.state, operation)
  return formatUnitList([row.label, ...visible, ...(visible.includes(state) ? [] : [state])])
}

/** A row's state as a word, the active one worded by operation. */
export function stateWord(state: TreeRowState, operation: TransferTreeOperation): string {
  switch (state) {
    case 'waiting':
      return tString('fileOperations.transferTree.state.waiting')
    case 'active':
      return activeWord(operation)
    case 'done':
      return tString('fileOperations.transferTree.state.done')
    case 'skipped':
      return tString('fileOperations.transferTree.state.skipped')
    case 'failed':
      return tString('fileOperations.transferTree.state.failed')
  }
}

function activeWord(operation: TransferTreeOperation): string {
  switch (operation) {
    case 'copy':
      return tString('fileOperations.transferTree.state.copying')
    case 'move':
      return tString('fileOperations.transferTree.state.moving')
    case 'compress':
      return tString('fileOperations.transferTree.state.compressing')
  }
}

function filesTotalText(count: number): string {
  return tString('fileOperations.transferTree.filesTotal', { countText: formatInteger(count), count })
}

function skippedText(count: number): string {
  return tString('fileOperations.transferTree.skippedCount', { countText: formatInteger(count), count })
}

function failedText(count: number): string {
  return tString('fileOperations.transferTree.failedCount', { countText: formatInteger(count), count })
}

/** A size as `<Size rounded>` writes it, for text that can't hold markup. */
function plainSize(bytes: number): string {
  return sizeDisplayParts(bytes, true)
    .map((part) => part.value)
    .join('')
}

/** The visible size: "4.0 MB", or "1.2 GB / 4.0 GB" while it's at work. */
function sizeLine(done: number | null, total: number): string {
  return done === null ? plainSize(total) : `${plainSize(done)} / ${plainSize(total)}`
}

/**
 * Every row's full path relative to the transfer's root, for its tooltip.
 * `ancestors` are the open rows above the window's first row; with them, each
 * row's parents are the nearest earlier rows one level up.
 */
export function rowPaths(rows: readonly TransferTreeRow[], ancestors: readonly TransferTreeAncestor[]): string[] {
  const trail = ancestors.map((ancestor) => ancestor.row.label)
  return rows.map((row) => {
    trail.length = Math.min(trail.length, row.depth)
    const path = [...trail, row.label].join('/')
    if (row.kind === 'folder') trail[row.depth] = row.label
    return path
  })
}
