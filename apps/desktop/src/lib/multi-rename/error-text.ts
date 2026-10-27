/**
 * The sheet's error line: every typed refusal the backend can answer, in words.
 */
import { tString } from '#lib/intl/messages.svelte.js'
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

export function errorText(error: MultiRenameError): string {
  switch (error.type) {
    case 'spec':
      return specErrorText(error.error)
    case 'gone':
    case 'sessionClosed':
      return tString('multiRename.error.gone')
    case 'selectionChanged':
      return tString('multiRename.selectionChanged')
    case 'nothingToRename':
      return tString('multiRename.error.nothingToRename')
    case 'notConnected':
      return tString('multiRename.error.notConnected')
    // A queued rename waits on `stillReading` rather than showing it; worded like a stale preview otherwise.
    case 'previewOutOfDate':
    case 'stillReading':
      return tString('multiRename.error.previewOutOfDate')
    case 'readOnly':
      return readOnlyText(error.place)
    case 'timedOut':
      return tString('multiRename.error.timedOut')
    case 'couldntWriteNames':
      return tString('multiRename.error.couldntWriteNames')
    case 'namesFileGone':
      return tString('multiRename.results.gone')
    case 'couldntStart':
    case 'internal':
      return tString('multiRename.error.couldntStart')
  }
}
