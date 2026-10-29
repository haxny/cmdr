/**
 * The name mask's two languages (`MaskSyntax`): Cmdr's placeholders and foobar2000 title formatting
 * (`src-tauri/src/multi_rename/foobar/`). Telling a pasted foobar pattern apart, whether one reads tags,
 * and where the engine's byte offset sits in the field. Pure.
 */
import type { MaskSyntax, MultiRenameSpec } from '#lib/ipc/bindings.js'
import { usesMediaFields } from './media-token'

/** A `%field%`: a letter first, so `100%` or `50% to 60%` stays plain text. */
const FOOBAR_FIELD = /%[a-z][a-z0-9 ]*%/i
/** A `$function(`. */
const FOOBAR_CALL = /\$[a-z][a-z0-9]*\(/i

/** Whether `mask` reads as a foobar2000 pattern: it has a `%field%` or a `$function(`. */
export function looksLikeFoobar(mask: string): boolean {
  return FOOBAR_FIELD.test(mask) || FOOBAR_CALL.test(mask)
}

/** The foobar fields that come from the file's name, not its tags (`foobar/fields.rs`). */
const NAME_FIELDS = new Set(['filename', 'directoryname'])

/**
 * Whether a foobar pattern names a tag field, so its rows wait for the tags. `'…'` is literal text, so a
 * field inside quotes doesn't count. The engine's `Pattern::uses_media` decides the preview; this only
 * says whether the sheet follows the read.
 */
export function foobarUsesMedia(mask: string): boolean {
  const unquoted = mask.replace(/'[^']*'/g, '')
  return [...unquoted.matchAll(/%([^%]*)%/g)].some(([, name]) => !NAME_FIELDS.has(name.toLowerCase()))
}

/** Whether the name mask in `syntax` reads files' EXIF or tags. */
export function maskUsesMedia(mask: string, syntax: MaskSyntax): boolean {
  return syntax === 'foobar' ? foobarUsesMedia(mask) : usesMediaFields(mask)
}

/** The field index (UTF-16, as an input's caret counts) of the engine's `byteOffset` (UTF-8) into `text`. */
export function utf16Offset(text: string, byteOffset: number): number {
  let bytes = 0
  let index = 0
  for (const char of text) {
    if (bytes >= byteOffset) return index
    const code = char.codePointAt(0) ?? 0
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4
    index += char.length
  }
  return index
}

/** The foobar fields the sheet offers to insert in foobar mode, the ones audiobook names use most. */
export const FOOBAR_INSERTS: readonly string[] = [
  '%album artist%',
  '%album%',
  '%performer%',
  '%tracknumber%',
  '%title%',
]

/** A spec after an edit, and what the edit did to the "switched to foobar" notice. */
export interface PatchedSpec {
  spec: MultiRenameSpec
  /** `true`: the edit switched the mask to foobar on its own; `false`: the user chose a syntax; `null`: neither. */
  switched: boolean | null
}

/**
 * Applies `patch` to `spec`. A name mask that turns into a foobar pattern (it didn't look like one before,
 * it does now) while the syntax is Cmdr's switches the syntax to foobar, so a pasted pattern works as is.
 * Only on that turn: after the user switches back, typing on in the same pattern stays Cmdr.
 */
export function patchSpec(spec: MultiRenameSpec, patch: Partial<MultiRenameSpec>): PatchedSpec {
  const next = { ...spec, ...patch }
  if (patch.maskSyntax !== undefined) return { spec: next, switched: false }
  const turned =
    patch.nameMask !== undefined &&
    (next.maskSyntax ?? 'cmdr') === 'cmdr' &&
    !looksLikeFoobar(spec.nameMask) &&
    looksLikeFoobar(patch.nameMask)
  return turned ? { spec: { ...next, maskSyntax: 'foobar' }, switched: true } : { spec: next, switched: null }
}
