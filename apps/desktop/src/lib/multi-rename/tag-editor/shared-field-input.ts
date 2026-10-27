/**
 * The pure half of the shared-fields table: text typed into a field ⇄ the values the field holds, and a track's
 * length as text. The «multiple values» rule lives in the components (an untouched row sends nothing); this only
 * says what a row looks like and what typed text means.
 *
 * ❗ A multi-value field whose single value contains "; " is split into two values when its row is committed again:
 * the text form can't tell the two apart. That is inherent to showing several values in one box.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import type { SharedValue, TagField, ValueCount } from '#lib/ipc/bindings.js'
import type { TagEdit } from '#lib/tauri-commands/index.js'

/** What joins the values of a multi-value field in its text. */
export const MULTI_SEPARATOR = '; '

/** The fields that hold several values (the engine's `TagField::is_multi`). */
const MULTI_FIELDS: ReadonlySet<TagField> = new Set<TagField>([
  'artist',
  'albumArtist',
  'performer',
  'composer',
  'genre',
  'translator',
  'www',
])

/** Comment and Description are prose: a textarea, not a line. */
export function isMultiLineField(field: TagField): boolean {
  return field === 'comment' || field === 'description'
}

/**
 * The values `text` stands for. A multi-value field splits on "; " and trims each value; any other field keeps its
 * text as one value (trimmed, except the prose fields Comment and Description, which are kept as typed). Nothing left (blank text) is `[]`, which removes the field.
 */
export function parseFieldText(field: TagField, text: string): string[] {
  if (text.trim() === '') return []
  if (isMultiLineField(field)) return [text]
  if (!MULTI_FIELDS.has(field)) return [text.trim()]
  return text
    .split(MULTI_SEPARATOR)
    .map((value) => value.trim())
    .filter((value) => value !== '')
}

/** A shared value as text. `multiple` has no text: the row shows its «multiple values» placeholder instead. */
export function formatFieldValue(value: SharedValue): { text: string; multiple: boolean } {
  if (value === 'empty') return { text: '', multiple: false }
  if (value === 'multiple') return { text: '', multiple: true }
  return { text: value.same.join(MULTI_SEPARATOR), multiple: false }
}

/**
 * What the box of a «multiple values» row holds, as foobar2000 shows it: the marker, then every value the files
 * carry (flattened, each once, in the order of the counts) joined with "; ". The box keeps all of it; a long line
 * is cut with … by the display (CSS), never in the text.
 */
export function multipleText(choices: readonly ValueCount[]): string {
  const marker = tString('multiRename.tags.multipleValues')
  const values = [...new Set(choices.flatMap((choice) => choice.values))]
  return values.length === 0 ? marker : `${marker} ${values.join(MULTI_SEPARATOR)}`
}

/**
 * What the user typed into a shared row's box, as an edit. `null` when nothing is to be sent: the text is as it was
 * (`original`), or the «multiple values» marker is still in it (the per-file values stay). Without the marker, the
 * text is what every file gets (`setAll`), and an empty box clears the field, as Clear does.
 */
export function parseSharedEdit(field: TagField, original: string, typed: string): TagEdit | null {
  if (typed === original) return null
  if (typed.includes(tString('multiRename.tags.multipleValues'))) return null
  return { kind: 'setAll', field, values: parseFieldText(field, typed) }
}

/** A track length as `m:ss`, or `h:mm:ss` from an hour on; blank when the file has none. */
export function formatTrackLength(durationMs: number | null): string {
  if (durationMs === null) return ''
  const totalSeconds = Math.floor(durationMs / 1000)
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = String(totalSeconds % 60).padStart(2, '0')
  return hours > 0 ? `${String(hours)}:${String(minutes).padStart(2, '0')}:${seconds}` : `${String(minutes)}:${seconds}`
}
