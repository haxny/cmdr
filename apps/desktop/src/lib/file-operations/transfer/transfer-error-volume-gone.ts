/**
 * What the transfer error dialog says when a volume went away mid-transfer: a drive
 * or device that left (`device_disconnected`), and a server whose connection was
 * lost (`server_connection_lost`). Both are about where the person's files are and
 * what to check, so they live together, split out of `transfer-error-messages.ts`.
 * DETAILS § "What the error dialog says when a drive was pulled" and § "What it says
 * when a server's connection was lost".
 */
import type { WriteOperationError, TransferOperationType } from '#lib/file-explorer/types.js'
import type { ProgressAtStop } from '#lib/tauri-commands/index.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { escapeHtml } from '#lib/tooltip/tooltip.js'
import { formatDuration, seconds } from '#lib/units/index.js'
import { w, type FriendlyErrorMessage } from './transfer-error-catalog'

/**
 * The two outcomes of a volume that went away mid-transfer: a drive or device that
 * left, and a server whose connection was lost. `getUserFriendlyMessage` asks it before
 * its own switch. `null` means "not one of mine".
 */
export function volumeGoneMessage(
  error: WriteOperationError,
  operationType: TransferOperationType,
  progressAtStop: ProgressAtStop | null,
): FriendlyErrorMessage | null {
  switch (error.type) {
    case 'device_disconnected':
      return deviceDisconnectedMessage(error, operationType, progressAtStop)
    case 'server_connection_lost':
      return serverConnectionLostMessage(error, operationType)
    default:
      return null
  }
}

/**
 * The sentence for a drive that left in the middle of a transfer.
 *
 * Three facts, in the order a worried person needs them: which drive went, how
 * far the transfer got, and where their files are now. The backend names the
 * side (it captured both volumes when the transfer started, because a volume
 * that vanishes can't be looked up afterwards), so ❌ nothing here guesses one
 * from a path.
 *
 * Falls back to the volume-agnostic sentence for a backend session that dropped
 * with no typed side (MTP, SMB), and for a transfer that stopped before the
 * counts existed: a sentence reading "copied 0 of 0 files" would be worse than
 * one that doesn't count at all.
 */
function deviceDisconnectedMessage(
  error: Extract<WriteOperationError, { type: 'device_disconnected' }>,
  operationType: TransferOperationType,
  progress: ProgressAtStop | null,
): FriendlyErrorMessage {
  const title = w('deviceDisconnected.title')
  const suggestion = w('deviceDisconnected.suggestion')
  const side = error.side
  const counted = progress !== null && progress.filesTotal > 0
  // A move that stops keeps every original, so its destination sentence reports
  // no partial progress: there isn't any to report.
  const needsCounts = !(side?.role === 'destination' && operationType === 'move')
  const sidedOperation = operationType === 'copy' || operationType === 'move'
  if (side && sidedOperation && (counted || !needsCounts)) {
    return {
      title,
      message: w(`deviceDisconnected.sided.${side.role}.${operationType}`, {
        volumeName: escapeHtml(side.volumeName),
        counterpart: escapeHtml(side.counterpartName),
        done: formatInteger(progress?.filesDone ?? 0),
        total: formatInteger(progress?.filesTotal ?? 0),
      }),
      suggestion,
    }
  }
  return { title, message: w(`deviceDisconnected.message.${operationType}`), suggestion }
}

/**
 * The sentence for a server whose connection ended a transfer.
 *
 * The connection is the cause, so the server heads the title; the file is context,
 * by name only (the full path is in the technical details). The backend's outage
 * wait decided it from typed facts, ❌ nothing here infers "server" from a path.
 * `gaveUpAfterSecs` picks between "Cmdr waited this long, then stopped" and "it
 * dropped" (no wait this time: a sign-in needed, or a link that kept dropping).
 * Only copy and move reach it for now, so any other operation reads as a copy.
 */
function serverConnectionLostMessage(
  error: Extract<WriteOperationError, { type: 'server_connection_lost' }>,
  operationType: TransferOperationType,
): FriendlyErrorMessage {
  const verb = operationType === 'move' ? 'move' : 'copy'
  const volumeName = escapeHtml(error.volumeName)
  const fileName = escapeHtml(lastPathSegment(error.path))
  const message =
    error.gaveUpAfterSecs === null
      ? w(`serverConnectionLost.message.dropped.${verb}`, { volumeName, fileName })
      : w(`serverConnectionLost.message.gaveUp.${verb}`, {
          volumeName,
          fileName,
          duration: formatDuration(seconds(error.gaveUpAfterSecs)),
        })
  return {
    title: w('serverConnectionLost.title', { volumeName }),
    message,
    suggestion: w(`serverConnectionLost.suggestion.${verb}`),
  }
}

/** The last segment of a backend path: the name a person recognizes the file by. */
function lastPathSegment(path: string): string {
  return path.split('/').findLast((segment) => segment !== '') ?? path
}
