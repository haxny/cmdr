/**
 * Single entry point for opening the error-report dialog, in either of its two modes.
 *
 * `openErrorReportDialog({ from, initialNote? })` is the compose path. The Help menu calls
 * it directly; every surface that offers a report for something that just went wrong or
 * got slow goes through `openErrorReportFor(incident)`, which words the note from the
 * typed incident (`report-incident.ts`). `openErrorReportDialogForAutoSentReport()` is the
 * amend path, reached only from the Flow B auto-sent toast: same dialog, but it shows the
 * report that already shipped and adds the user's note to THAT report instead of uploading
 * a second one.
 *
 * The dialog mounts once, in the main window (`(main)/+layout.svelte` calls
 * `hostErrorReportDialog()`). A window that doesn't host it (the queue window, Settings)
 * asks the main window over `open-error-report` instead of flipping a store nobody renders.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'
import type { ErrorReportEntry } from '#lib/ipc/bindings.js'
import { onErrorReportRequested, requestErrorReportInMainWindow } from '#lib/tauri-commands/error-reporter.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { recordBreadcrumb } from './breadcrumbs'
import { reportEntryFor, reportNoteFor, slowBreadcrumbFor, type ReportIncident } from './report-incident'

const log = getAppLogger('errorReporter')

/** `compose` builds and uploads a new report; `amend` adds a note to the auto-sent one. */
export type ErrorReportMode = 'compose' | 'amend'

interface FlowState {
  open: boolean
  initialNote: string
  mode: ErrorReportMode
}

export const errorReportFlow = $state<FlowState>({
  open: false,
  initialNote: '',
  mode: 'compose',
})

/** What a compose open carries: the surface that offered it, and the note to start from. */
export interface ErrorReportRequest {
  from: ErrorReportEntry
  initialNote?: string
}

/** True in the one webview that renders the dialog. */
let hostedHere = false

function openHere({ from, initialNote = '' }: ErrorReportRequest): void {
  errorReportFlow.initialNote = initialNote
  errorReportFlow.mode = 'compose'
  errorReportFlow.open = true
  recordBreadcrumb({ type: 'errorReportDialogOpened', hasInitialNote: initialNote !== '', from })
}

/**
 * Makes this webview the dialog's host: compose opens land here, and so do the ones other
 * windows ask for. Returns the teardown. Called once, by the layout that mounts the dialog.
 */
export function hostErrorReportDialog(): () => void {
  hostedHere = true
  let stopped = false
  let unlisten: (() => void) | undefined
  onErrorReportRequested((request) => {
    openHere(request)
    // The person clicked in another window; the dialog is here.
    getCurrentWindow()
      .setFocus()
      .catch((error: unknown) => {
        log.warn('Could not focus the main window for an error report: {error}', { error: String(error) })
      })
  })
    .then((fn) => {
      if (stopped) fn()
      else unlisten = fn
    })
    .catch((error: unknown) => {
      log.warn('Could not listen for error-report requests from other windows: {error}', { error: String(error) })
    })
  return () => {
    stopped = true
    hostedHere = false
    unlisten?.()
  }
}

/**
 * Open the error-report dialog in compose mode, pre-filled with `initialNote` when given.
 * From a window that doesn't host the dialog, this asks the main window to open it.
 */
export function openErrorReportDialog(request: ErrorReportRequest): void {
  if (hostedHere) {
    openHere(request)
    return
  }
  requestErrorReportInMainWindow({ from: request.from, initialNote: request.initialNote ?? '' }).catch(
    (error: unknown) => {
      // Never error level: that would auto-send a report about failing to open one.
      log.warn('Could not ask the main window to open the error report: {error}', { error: String(error) })
    },
  )
}

/**
 * Offer a report for something that just went wrong or got slow. A slow incident also
 * records its slowness first, so the bundle carries it with the duration the person saw.
 */
export function openErrorReportFor(incident: ReportIncident): void {
  const now = Date.now()
  if (incident.kind === 'slow') recordBreadcrumb(slowBreadcrumbFor(incident, now))
  openErrorReportDialog({ from: reportEntryFor(incident), initialNote: reportNoteFor(incident, now) })
}

/**
 * Open the dialog on the report Flow B already auto-sent, so a note joins THAT report.
 * Nothing here can upload a second bundle: the dialog's amend mode calls
 * `amendErrorReport`, never `sendErrorReport`.
 */
export function openErrorReportDialogForAutoSentReport(): void {
  errorReportFlow.initialNote = ''
  errorReportFlow.mode = 'amend'
  errorReportFlow.open = true
  recordBreadcrumb({ type: 'errorReportAmendDialogOpened' })
}

export function closeErrorReportDialog(): void {
  errorReportFlow.open = false
  errorReportFlow.initialNote = ''
  errorReportFlow.mode = 'compose'
  recordBreadcrumb({ type: 'errorReportDialogClosed' })
}
