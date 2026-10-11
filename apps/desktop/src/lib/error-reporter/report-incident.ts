/**
 * What a "Send error report…" offer is about, as typed facts.
 *
 * Every surface that offers a report (a toast, the transfer error dialog, the
 * error pane, a refused create, a failed queue row, a slowness notice) hands
 * the shared `ReportOffer.svelte` one `ReportIncident`. This module turns it
 * into the two things the report needs: the surface it came from (the
 * dialog-opened breadcrumb's `from`) and the note the dialog opens pre-filled
 * with. The note is words for the person to keep or edit; the typed kinds also
 * land in its last line as a short reference (`copy/insufficient_space`), so a
 * report reads the same in triage whatever language it was written in.
 *
 * ❌ Nothing here reads an error MESSAGE to decide anything: every kind is the
 * typed tag the backend sent (`WriteOperationError['type']`, a listing reason,
 * a `MutationError` type, an `UpgradeFailure`).
 */

import type {
  BreadcrumbEvent,
  ErrorReportEntry,
  ListingErrorReason,
  MutationError,
  SlowOperation,
  SlowWaitingOn,
  UpgradeFailure,
  WriteOperationError,
  WriteOperationType,
} from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import { formatDuration, seconds } from '$lib/units'

/** A file operation that stopped before it was done. */
export interface OperationFailedIncident {
  kind: 'operationFailed'
  from: Extract<ErrorReportEntry, 'transferError' | 'queueFailure'>
  operation: WriteOperationType
  error: WriteOperationError['type']
  /** The pipeline's plain-text title for it, the headline the person saw. */
  title: string
}

/** Something has made no visible progress for a while. */
export interface SlowIncident {
  kind: 'slow'
  operation: SlowOperation
  /** When it stopped moving, in `Date.now()` time. The note measures from here at open time. */
  since: number
  /** Which side a stalled transfer waits on; `null` for a create. */
  waitingOn: SlowWaitingOn | null
}

export type ReportIncident =
  /** An already-worded failure (a plain error toast, an update failure): its text is the note. */
  | { kind: 'message'; from: Extract<ErrorReportEntry, 'errorToast' | 'updateFailure'>; text: string }
  | OperationFailedIncident
  /** The collapsed notice for a burst of backgrounded failures. */
  | { kind: 'operationsFailed'; count: number }
  /** A folder that couldn't be listed. `reason` is null for a failure that carried no typed reason. */
  | { kind: 'listingFailed'; reason: ListingErrorReason['reason'] | null; title: string }
  /** A new folder or file the volume refused while its dialog was open. */
  | { kind: 'createRefused'; entry: 'folder' | 'file'; refusal: MutationError['type']; reason: string }
  /** A direct SMB connection that didn't come about. */
  | { kind: 'connectionFailed'; reason: UpgradeFailure; text: string }
  | SlowIncident

// `SlowOperation` is the breadcrumb's own copy of `WriteOperationType`; this fails to
// compile the moment the two vocabularies drift apart.
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false
const _slowOperationMirrorsWriteOperationType: Same<SlowOperation, WriteOperationType> = true

/** Which surface the report came from, for the dialog-opened breadcrumb. */
export function reportEntryFor(incident: ReportIncident): ErrorReportEntry {
  switch (incident.kind) {
    case 'message':
    case 'operationFailed':
      return incident.from
    case 'operationsFailed':
      return 'queueFailure'
    case 'listingFailed':
      return 'listingFailure'
    case 'createRefused':
      return 'createRefusal'
    case 'connectionFailed':
      return 'connectionFailure'
    case 'slow':
      return 'slowOperation'
  }
}

/** Whole seconds since a slow incident stopped moving, as of `now`. */
export function slowSecondsAt(incident: SlowIncident, now: number): number {
  return Math.max(0, Math.round((now - incident.since) / 1000))
}

/** The breadcrumb that puts a slowness notice into the report bundle. */
export function slowBreadcrumbFor(
  incident: SlowIncident,
  now: number,
): Extract<BreadcrumbEvent, { type: 'operationSlow' }> {
  return {
    type: 'operationSlow',
    operation: incident.operation,
    stillForSecs: slowSecondsAt(incident, now),
    waitingOn: incident.waitingOn,
  }
}

function reference(code: string): string {
  return tString('errorReporter.prefill.reference', { code })
}

function waitingLine(waitingOn: SlowWaitingOn | null): string | null {
  switch (waitingOn) {
    case 'destination':
      return tString('fileOperations.transferProgress.stallWaitingDestination')
    case 'source':
      return tString('fileOperations.transferProgress.stallWaitingSource')
    case 'unknown':
    case null:
      return null
  }
}

function slowNote(incident: SlowIncident, now: number): string[] {
  const headline = tString('errorReporter.prefill.slow', {
    operation: tString('queue.row.label', { type: incident.operation }),
    duration: formatDuration(seconds(slowSecondsAt(incident, now))),
  })
  const code = [incident.operation, 'slow', incident.waitingOn].filter((part) => part !== null).join('/')
  return [headline, waitingLine(incident.waitingOn), reference(code)].filter((line) => line !== null)
}

/**
 * The note the report dialog opens with. `now` is when the person asked, so a
 * slow incident's duration is the one they were looking at.
 */
export function reportNoteFor(incident: ReportIncident, now: number = Date.now()): string {
  const lines = ((): string[] => {
    switch (incident.kind) {
      case 'message':
        return [incident.text]
      case 'operationFailed':
        return [incident.title, reference(`${incident.operation}/${incident.error}`)]
      case 'operationsFailed':
        return [
          tString('queue.failureToast.summary', {
            count: incident.count,
            countText: formatInteger(incident.count),
          }),
        ]
      case 'listingFailed':
        return incident.reason === null ? [incident.title] : [incident.title, reference(`listing/${incident.reason}`)]
      case 'createRefused':
        return [
          incident.reason,
          reference(`${incident.entry === 'folder' ? 'create_folder' : 'create_file'}/${incident.refusal}`),
        ]
      case 'connectionFailed':
        return [incident.text, reference(`smb/${incident.reason}`)]
      case 'slow':
        return slowNote(incident, now)
    }
  })()
  return lines.join('\n')
}
