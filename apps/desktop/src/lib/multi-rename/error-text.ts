/**
 * The sheet's error line: every typed refusal the backend can answer, in words.
 */
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { MaskError, SpecError } from '#lib/ipc/bindings.js'
import type { MultiRenameError } from '#lib/tauri-commands/index.js'

/** Why a mask doesn't parse, in words; the Tags tab's Format panel words its mask the same way. */
export function maskErrorText(error: MaskError): string {
  return error.type === 'unclosed'
    ? tString('multiRename.error.unclosed')
    : tString('multiRename.error.unknown', { placeholder: error.placeholder })
}

function specErrorText(error: SpecError): string {
  if (error.type === 'badRegex') return tString('multiRename.error.badRegex')
  return maskErrorText(error.error)
}

function readOnlyText(place: Extract<MultiRenameError, { type: 'readOnly' }>['place']): string {
  if (place === 'gitPortal') return tString('fileExplorer.readOnly.gitPortalMessage')
  if (place === 'resticSnapshots') return tString('fileExplorer.readOnly.resticMessage')
  return tString('multiRename.error.readOnly')
}

/** The refusals worded by one fixed message. */
const PLAIN_TEXT: Record<Exclude<MultiRenameError['type'], 'spec' | 'readOnly'>, MessageKey> = {
  gone: 'multiRename.error.gone',
  sessionClosed: 'multiRename.error.gone',
  selectionChanged: 'multiRename.selectionChanged',
  nothingToRename: 'multiRename.error.nothingToRename',
  notConnected: 'multiRename.error.notConnected',
  // A queued rename waits on `stillReading` rather than showing it; worded like a stale preview otherwise.
  previewOutOfDate: 'multiRename.error.previewOutOfDate',
  stillReading: 'multiRename.error.previewOutOfDate',
  timedOut: 'multiRename.error.timedOut',
  couldntWriteNames: 'multiRename.error.couldntWriteNames',
  namesFileGone: 'multiRename.results.gone',
  // The sort sends the session's own names, so this is a bug if it shows; worded like any internal failure.
  notAPermutation: 'multiRename.error.couldntStart',
  couldntStart: 'multiRename.error.couldntStart',
  internal: 'multiRename.error.couldntStart',
}

export function errorText(error: MultiRenameError): string {
  if (error.type === 'spec') return specErrorText(error.error)
  if (error.type === 'readOnly') return readOnlyText(error.place)
  return tString(PLAIN_TEXT[error.type])
}
