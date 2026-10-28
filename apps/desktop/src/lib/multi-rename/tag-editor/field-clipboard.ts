/**
 * ⌘C / ⌘X / ⌘V on a field (`TagsContent.clipboard`), the pure half: what a shared row copies, and the values pasted
 * text stands for in a field. Plain text only.
 */

import type { SharedRow, TagField } from '#lib/ipc/bindings.js'
import { MULTI_SEPARATOR, isMultiLineField, isMultiValueField, parseFieldText } from './shared-field-input'

/**
 * What an Album row copies: its value as the row shows it, a «multiple values» row's distinct values without the
 * marker (or the «…and N more» tail), and an empty field as an empty string (so the old clipboard never stays).
 */
export function sharedCopyText(row: SharedRow): string {
  if (row.value === 'empty') return ''
  if (row.value === 'multiple')
    return [...new Set(row.choices.flatMap((choice) => choice.values))].join(MULTI_SEPARATOR)
  return row.value.same.join(MULTI_SEPARATOR)
}

/**
 * The values `text` from the clipboard stands for in `field`: one trailing line break dropped; Comment and
 * Description keep their lines; in any other field the lines are joined with "; " when it holds several values, else
 * the first line is taken. Then as typed (`parseFieldText`): blank text removes the field.
 */
export function pastedValues(field: TagField, text: string): string[] {
  const lines = text.replace(/\r\n?/g, '\n').replace(/\n$/, '')
  if (isMultiLineField(field)) return parseFieldText(field, lines)
  const split = lines.split('\n')
  return parseFieldText(field, isMultiValueField(field) ? split.join(MULTI_SEPARATOR) : (split[0] ?? ''))
}
