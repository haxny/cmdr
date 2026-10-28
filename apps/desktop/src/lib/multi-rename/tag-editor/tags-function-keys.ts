/**
 * The Tags window's function keys: which tag command each slot of the bar carries (`TagsFunctionKeyBar`), and which
 * keys the window's keydown answers (`TagsWindow`). Pure, so the slots are pinned without mounting anything. Every id
 * is a `Tags window` registry command (`commands/sources/tags-window.ts`), so the bar's chips and the keys read the
 * registry, never hardcoded F-keys. Those keys are fixed (`fixedKey`): Settings doesn't rebind them yet.
 */
import type { CommandId } from '#lib/commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { TagEditorView } from '#lib/tauri-commands/index.js'
import { createKeyRoadEcho } from '../preset-keys'

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

/** Every command the window runs: the bar's, plus ⌘Z. (Tab, `tags.switchPane`, is the panes' own.) */
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

/** The bar's commands that need the tags to be editable. */
const EDITING_COMMANDS: readonly TagsBarCommandId[] = [
  'tags.edit',
  'tags.replace',
  'tags.case',
  'tags.format',
  'tags.cleanUp',
  'tags.fromName',
  'tags.number',
  'tags.clearField',
  'tags.write',
]

/** The bar's commands that can act on `view`: nothing before the tags are read; read-only leaves Play and the player. */
export function enabledTagsCommands(view: TagEditorView | null): ReadonlySet<TagsBarCommandId> {
  if (view === null) return new Set()
  const on: TagsBarCommandId[] = ['tags.openInPlayer']
  if (view.tracks.some((track) => track.readError === null)) on.push('tags.play')
  if (view.editable) {
    on.push(...EDITING_COMMANDS)
    if (view.tracks.length > 1) on.push('tags.editNumber')
    if (view.extras.length > 0) on.push('tags.removeExtras')
  }
  return new Set(on)
}

/**
 * The commands a main-menu key may run in the window (`TagsMenuKey`, `tag_editor::window::menu_key_command_of`).
 * ❌ Never Tab: the page sees it anyway, and in a dialog or a field it must move focus, not switch panes.
 */
const MENU_KEY_COMMANDS: ReadonlySet<string> = new Set<TagsCommandId>([
  'tags.edit',
  'tags.editNumber',
  'tags.play',
  'tags.replace',
  'tags.case',
  'tags.format',
  'tags.fromName',
  'tags.number',
  'tags.clearField',
  'tags.removeExtras',
])

/**
 * The Tags command a main-menu key press means, or `null`. The menu item runs `menuCommandId` in the main window; its
 * first key is what the item's accelerator holds. It means `commandId` only while that key is one of `commandId`'s, so
 * a file command rebound to another key never runs a tag tool. `shortcutsOf` is the shortcuts store's lookup.
 */
export function menuKeyCommand(
  key: { commandId: string; menuCommandId: string },
  shortcutsOf: (id: CommandId) => readonly string[],
): TagsCommandId | null {
  if (!MENU_KEY_COMMANDS.has(key.commandId)) return null
  const commandId = key.commandId as TagsCommandId
  const itemKey = shortcutsOf(key.menuCommandId as CommandId).at(0)
  return itemKey !== undefined && shortcutsOf(commandId).includes(itemKey) ? commandId : null
}

/** How a key press reached the window: its keydown, or the native menu's accelerator. */
export type TagsKeyRoad = 'keyboard' | 'menu'

/**
 * Tells one key press that arrived by both roads (AppKit may fire the menu accelerator AND let the page see the key)
 * from two presses, per command: the other road within the echo window is the echo. A repeat on the same road counts.
 */
export function createTagsKeyEcho(now: () => number = Date.now): (id: TagsCommandId, road: TagsKeyRoad) => boolean {
  const roads = new Map<TagsCommandId, ReturnType<typeof createKeyRoadEcho>>()
  return (id, road) => {
    let echo = roads.get(id)
    if (echo === undefined) {
      echo = createKeyRoadEcho(now)
      roads.set(id, echo)
    }
    return echo(road)
  }
}

/** A box that holds typed text (a checkbox or a button doesn't: its `selectionStart` is `null`). */
export function textBoxOf(el: Element | null): HTMLInputElement | HTMLTextAreaElement | null {
  if (el instanceof HTMLTextAreaElement) return el
  return el instanceof HTMLInputElement && el.selectionStart !== null ? el : null
}
