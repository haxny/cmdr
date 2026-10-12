/**
 * A failed update download, carried across the throw with its typed reason so the updater can pick the log level
 * (`downloadInstallLogLevel` in `updater.svelte.ts`). Lives outside `#lib/tauri-commands` so the updater's tests,
 * which mock that barrel, still get the real class.
 */
import type { UpdateDownloadError } from '#lib/ipc/bindings.js'
import { serverRequestDiagnostic } from '#lib/error-messages/server-request.js'
import { TypedFailure } from '#lib/ipc/typed-failure.js'

/** An `Error` that still carries the backend's typed download failure, so the updater can pick its log level. */
export class UpdateDownloadFailure extends TypedFailure<UpdateDownloadError> {
  constructor(failure: UpdateDownloadError) {
    super(failure, `update download: ${downloadDiagnostic(failure)}`)
    this.name = 'UpdateDownloadFailure'
  }
}

function downloadDiagnostic(failure: UpdateDownloadError): string {
  switch (failure.type) {
    case 'request':
      return serverRequestDiagnostic(failure.failure)
    case 'signatureMismatch':
    case 'disk':
      return `${failure.type}: ${failure.detail}`
    case 'nothingOffered':
    case 'blockedByPolicy':
      return failure.type
  }
}
