/**
 * The error-report and feedback dialogs' "see what's sent" link: write the whole bundle as one
 * text file (the backend's `error_reporter/preview_file.rs`) and open it in the File viewer.
 */
import type { ReportPreviewFile } from '#lib/ipc/bindings.js'
import { openFileViewer } from '#lib/file-viewer/open-viewer.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'

const log = getAppLogger('reportPreview')

/**
 * Writes the preview through `write` and opens it in a viewer window. Returns the report id the
 * file was built under, so the caller can send under it, or `null` when no file was made (a
 * toast says so; the reason goes to the log).
 */
export async function openReportPreview(write: () => Promise<ReportPreviewFile>): Promise<string | null> {
  try {
    const file = await write()
    await openFileViewer(file.path)
    return file.id
  } catch (e) {
    log.warn("Couldn't open the report preview: {error}", { error: e })
    // No "Send error report…" on this toast: it's part of that very flow.
    addToast(tString('errorReporter.preview.unavailableToast'), { level: 'error', report: false })
    return null
  }
}
