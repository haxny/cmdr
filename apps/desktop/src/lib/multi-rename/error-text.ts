/**
 * The sheet's error line: every typed refusal the backend can answer, in words.
 */
import { tString } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { FoobarError, MaskError, SpecError } from '#lib/ipc/bindings.js'
import type { MultiRenameError } from '#lib/tauri-commands/index.js'
import { utf16Offset } from './mask-syntax'

/** Why a mask doesn't parse, in words; the Tags tab's Format panel words its mask the same way. */
export function maskErrorText(error: MaskError): string {
  return error.type === 'unclosed'
    ? tString('multiRename.error.unclosed')
    : tString('multiRename.error.unknown', { placeholder: error.placeholder })
}

/**
 * Where a foobar pattern breaks, as the name mask field's index (UTF-16), for its underline; `null` for
 * any other error. The engine counts bytes (`FoobarError.at`), which a Czech letter makes two of.
 */
export function patternErrorAt(error: MultiRenameError | null, nameMask: string): number | null {
  if (error?.type !== 'spec' || error.error.type !== 'pattern') return null
  return utf16Offset(nameMask, error.error.error.at)
}

/** Why a foobar pattern doesn't parse, and at which character (counted from 1, as the user reads it). */
function patternErrorText(error: FoobarError, nameMask: string): string {
  const before = nameMask.slice(0, utf16Offset(nameMask, error.at))
  const position = Array.from(new Intl.Segmenter().segment(before)).length + 1
  switch (error.type) {
    case 'unknownField':
      return tString('multiRename.error.foobar.unknownField', { name: error.name, position })
    case 'unknownFunction':
      return tString('multiRename.error.foobar.unknownFunction', { name: error.name, position })
    case 'badArguments':
      return tString('multiRename.error.foobar.badArguments', { function: error.function, position })
    case 'unclosed':
      return tString('multiRename.error.foobar.unclosed', { what: error.what, position })
    case 'tooDeep':
      return tString('multiRename.error.foobar.tooDeep', { position })
  }
}

function specErrorText(error: SpecError, nameMask: string): string {
  if (error.type === 'badRegex') return tString('multiRename.error.badRegex')
  if (error.type === 'pattern') return patternErrorText(error.error, nameMask)
  return maskErrorText(error.error)
}

function readOnlyText(place: Extract<MultiRenameError, { type: 'readOnly' }>['place']): string {
  if (place === 'gitPortal') return tString('fileExplorer.readOnly.gitPortalMessage')
  if (place === 'resticSnapshots') return tString('fileExplorer.readOnly.resticMessage')
  return tString('multiRename.error.readOnly')
}

/** The refusals worded by one fixed message. */
const PLAIN_TEXT: Record<Exclude<MultiRenameError['type'], 'spec' | 'readOnly' | 'tooManyTracks'>, MessageKey> = {
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
  foldersNeedLocalDisk: 'multiRename.tags.window.foldersNeedLocalDisk',
}

/** The error's words; `nameMask` places a foobar pattern's error in it. */
export function errorText(error: MultiRenameError, nameMask = ''): string {
  if (error.type === 'spec') return specErrorText(error.error, nameMask)
  if (error.type === 'readOnly') return readOnlyText(error.place)
  if (error.type === 'tooManyTracks') return tString('multiRename.tags.window.tooManyTracks', { limit: error.limit })
  return tString(PLAIN_TEXT[error.type])
}
