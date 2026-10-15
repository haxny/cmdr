/**
 * A refused or failed update install, carried across the throw with its typed reason so the updater can tell a policy
 * refusal (quiet) from a broken install (logged at error). Lives outside `#lib/tauri-commands` for the same reason as
 * `update-download-failure.ts`: the updater's tests mock that barrel and still need the real class.
 */
import type { UpdateInstallError } from '#lib/ipc/bindings.js'
import { TypedFailure } from '#lib/ipc/typed-failure.js'

/** An `Error` that still carries the backend's typed install failure. */
export class UpdateInstallFailure extends TypedFailure<UpdateInstallError> {
  constructor(failure: UpdateInstallError) {
    super(failure, describe(failure))
    this.name = 'UpdateInstallFailure'
  }
}

/** The log line for a failure: the backend's detail where it carries one. */
function describe(failure: UpdateInstallError): string {
  switch (failure.type) {
    case 'failed':
      return `update install: ${failure.detail}`
    case 'untrustedCodeSignature':
      return `update install refused, untrusted code signature: ${failure.detail}`
    default:
      return `update install ${failure.type}`
  }
}
