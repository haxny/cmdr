/** The From file name panel's pattern errors, in words; an unknown field is named. */
import { tString } from '#lib/intl/messages.svelte.js'
import type { PatternError } from '#lib/ipc/bindings.js'

/** A sample of the regular-expression mode, shown in the box's name and in the "no fields" error. */
export const REGEX_EXAMPLE = '(?<title>.+)'

export function patternErrorText(error: PatternError): string {
  switch (error.kind) {
    case 'badRegex':
      return tString('multiRename.tags.tools.name.badRegex')
    case 'unknownField':
      return tString('multiRename.tags.tools.name.unknownField', { name: error.name })
    case 'noFields':
      return tString('multiRename.tags.tools.name.noFields', { example: REGEX_EXAMPLE })
  }
}
