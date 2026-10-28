/**
 * The Tags window's function keys: which tag command each slot of the bar carries (`TagsFunctionKeyBar`), and which
 * keys the window's keydown answers (`TagsWindow`). Pure, so the slots are pinned without mounting anything. Every id
 * is a `Tags window` registry command (`commands/sources/tags-window.ts`), so the bar's chips and the keys are the
 * user's bindings, never hardcoded F-keys.
 */
import type { CommandId } from '#lib/commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'

/** The commands the bar offers, in the order their slots read. */
export const TAGS_BAR_COMMANDS = [
  'tags.edit',
  'tags.editNumber',
  'tags.play',
  'tags.openInPlayer',
  'tags.replace',
  'tags.case',
  'tags.format',
  'tags.cleanUp',
  'tags.fromName',
  'tags.number',
  'tags.clearField',
  'tags.removeExtras',
  'tags.write',
] as const satisfies readonly CommandId[]

export type TagsBarCommandId = (typeof TAGS_BAR_COMMANDS)[number]

/** Every command the window runs: the bar's, plus ⌘Z. */
export type TagsCommandId = TagsBarCommandId | 'tags.undo'

export interface TagsBarSlot {
  id: TagsBarCommandId
  label: MessageKey
}

/** The plain row, F2…F8 (slot `i` is `F${i + 2}` by default). */
export const TAGS_PLAIN_ROW: readonly TagsBarSlot[] = [
  { id: 'tags.edit', label: 'multiRename.tags.keys.edit' },
  { id: 'tags.play', label: 'multiRename.tags.keys.play' },
  { id: 'tags.replace', label: 'multiRename.tags.keys.replace' },
  { id: 'tags.format', label: 'multiRename.tags.keys.format' },
  { id: 'tags.fromName', label: 'multiRename.tags.keys.fromName' },
  { id: 'tags.number', label: 'multiRename.tags.keys.number' },
  { id: 'tags.clearField', label: 'multiRename.tags.keys.clearField' },
]

/** The Shift row, same slots; `null` is an F-key with no Shift action. */
export const TAGS_SHIFT_ROW: readonly (TagsBarSlot | null)[] = [
  { id: 'tags.editNumber', label: 'multiRename.tags.keys.editNumber' },
  { id: 'tags.openInPlayer', label: 'multiRename.tags.keys.openInPlayer' },
  { id: 'tags.case', label: 'multiRename.tags.keys.case' },
  { id: 'tags.cleanUp', label: 'multiRename.tags.keys.cleanUp' },
  null,
  null,
  { id: 'tags.removeExtras', label: 'multiRename.tags.keys.removeExtras' },
]

/** Write, last in both rows: the one slot that isn't an F-key. */
export const TAGS_WRITE_SLOT: TagsBarSlot = { id: 'tags.write', label: 'multiRename.tags.keys.write' }

/**
 * The window keydown's commands, in the order it asks. The pane keys (edit, play) come last: a focused pane answers
 * and claims them first, so the window sees them only from elsewhere (the other pane, the cover, no focus).
 */
export const TAGS_WINDOW_KEYS: readonly TagsCommandId[] = [
  'tags.write',
  'tags.undo',
  'tags.replace',
  'tags.case',
  'tags.format',
  'tags.cleanUp',
  'tags.fromName',
  'tags.number',
  'tags.clearField',
  'tags.removeExtras',
  'tags.openInPlayer',
  'tags.edit',
  'tags.editNumber',
  'tags.play',
]

/**
 * The commands a focused pane answers (and claims) itself. The window answers them elsewhere, except Enter on a
 * focused control, which is that control's (a button presses).
 */
export const PANE_KEYS: ReadonlySet<TagsCommandId> = new Set(['tags.edit', 'tags.editNumber', 'tags.play'])

/** A box that holds typed text (a checkbox or a button doesn't: its `selectionStart` is `null`). */
export function textBoxOf(el: Element | null): HTMLInputElement | HTMLTextAreaElement | null {
  if (el instanceof HTMLTextAreaElement) return el
  return el instanceof HTMLInputElement && el.selectionStart !== null ? el : null
}
