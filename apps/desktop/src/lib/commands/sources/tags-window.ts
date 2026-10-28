/**
 * The Tags window's own keys (`Tags window`, fork, behind `multiRename.tagEditor`). Pure data; the window's keydown
 * asks `eventMatchesCommand` for each (`multi-rename/tag-editor/TagsWindow.svelte`). Fixed for now: the window
 * reads the shortcuts store, but none of its keys is worth a rebinding yet. See `../command-registry.ts` for how the
 * scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/** Every entry shares these: the window answers them itself, never the dispatch spine. */
const tagsWindowKey = {
  scope: 'Tags window',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
  fixedKey: true,
} as const

export const tagsWindowCommands: CommandSource[] = [
  // Asks "Discard tag changes?" first when edits are unwritten.
  { id: 'tags.close', nameKey: 'commands.tagsClose.label', shortcuts: ['Escape'], ...tagsWindowKey },
]
