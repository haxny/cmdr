/**
 * Carrying a typed `ReportPreviewError` from the preview-file commands to `openReportPreview`.
 * Every variant reads the same to a person (one toast), so `detail` rides the diagnostic, for
 * the log only.
 */
import type { ReportPreviewError } from '#lib/ipc/bindings.js'
import { TypedFailure } from '#lib/ipc/typed-failure.js'

/** An `Error` that still carries the typed reason a preview file wasn't made. */
export class ReportPreviewFailure extends TypedFailure<ReportPreviewError> {
  constructor(failure: ReportPreviewError) {
    super(failure, `report preview: ${failure.type}: ${failure.detail}`)
    this.name = 'ReportPreviewFailure'
  }
}

/** Throws a wire `ReportPreviewError` as an `Error`, keeping the typed value. */
export function throwReportPreviewError(failure: ReportPreviewError): never {
  throw new ReportPreviewFailure(failure)
}
