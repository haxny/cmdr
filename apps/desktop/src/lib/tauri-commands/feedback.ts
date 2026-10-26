// "Send feedback" command

import { commands } from '#lib/ipc/bindings.js'
import type { ReportPreviewFile, SendFeedbackResult } from '#lib/ipc/bindings.js'
import { throwReportPreviewError } from '#lib/error-reporter/report-preview-error.js'

export type { SendFeedbackResult }

/**
 * Sends a beta tester's feedback text (plus an optional reply-to email when they ticked the
 * attach-email box, and the past five hours of logs when they ticked that one) to the api-server,
 * which stores it and files it on the reports board.
 *
 * `logsReportId` is the id a logs preview was built under (`writeFeedbackLogsPreviewFile`), so the
 * logs ship as the report the person looked at; omit it when they didn't look.
 *
 * Returns a typed result the caller branches on (`result.kind`), never a message string. On any
 * unexpected throw (which shouldn't happen, the command catches its own errors) we degrade to
 * `softFailure` so the UI always has a result to show.
 */
export async function sendFeedback(
  feedbackText: string,
  email: string | undefined,
  includeLogs: boolean,
  logsReportId?: string,
): Promise<SendFeedbackResult> {
  try {
    return await commands.sendFeedback(feedbackText, email ?? null, includeLogs, logsReportId ?? null)
  } catch {
    return { kind: 'softFailure' }
  }
}

/**
 * Writes the logs the feedback would carry as one text file for the File viewer. Returns its path
 * and the report id it was built under; pass that id back here to rewrite the same report, and to
 * `sendFeedback` to ship it.
 */
export async function writeFeedbackLogsPreviewFile(id?: string): Promise<ReportPreviewFile> {
  const res = await commands.writeFeedbackLogsPreviewFile(id ?? null)
  if (res.status === 'error') throwReportPreviewError(res.error)
  return res.data
}
