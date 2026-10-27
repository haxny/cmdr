/**
 * The pure half of the shared-fields table: text typed into a field ⇄ the values the field holds, and a track's
 * length as text. The «multiple values» rule lives in the components (an untouched row sends nothing); this only
 * says what a row looks like and what typed text means.
 */

import type { SharedValue, TagField } from '#lib/ipc/bindings.js'

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
 * text whole as one value. Nothing left (blank text) is `[]`, which removes the field.
 */
export function parseFieldText(field: TagField, text: string): string[] {
  if (text.trim() === '') return []
  if (!MULTI_FIELDS.has(field)) return [text]
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

/** A track length as `m:ss`, or `h:mm:ss` from an hour on; blank when the file has none. */
export function formatTrackLength(durationMs: number | null): string {
  if (durationMs === null) return ''
  const totalSeconds = Math.floor(durationMs / 1000)
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = String(totalSeconds % 60).padStart(2, '0')
  return hours > 0 ? `${String(hours)}:${String(minutes).padStart(2, '0')}:${seconds}` : `${String(minutes)}:${seconds}`
}
