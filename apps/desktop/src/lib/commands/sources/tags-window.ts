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
  // The two panes (Album | Tracks) switch like the main window's file panes (`pane.switch`). Answered by the pane
  // that has focus (`AlbumPane` / `TracksPane`), so Tab stays the browser's anywhere else in the window.
  { id: 'tags.switchPane', nameKey: 'commands.tagsSwitchPane.label', shortcuts: ['Tab'], ...tagsWindowKey },
  // Edits the cursor row's value in place, as F2 renames a file: the Album pane's value, the Tracks pane's Title.
  { id: 'tags.edit', nameKey: 'commands.tagsEdit.label', shortcuts: ['F2', 'Enter'], ...tagsWindowKey },
  { id: 'tags.editNumber', nameKey: 'commands.tagsEditNumber.label', shortcuts: ['⇧F2'], ...tagsWindowKey },
  // Plays or stops the Tracks pane's cursor row. Plain Space selects (`selection.toggle`), as in a file pane, so the
  // key is ⌥Space (macOS sends a non-breaking space for it, which `TracksPane` claims when it acts).
  {
    id: 'multiRename.playTrack',
    nameKey: 'commands.multiRenamePlayTrack.label',
    shortcuts: ['⌥Space'],
    ...tagsWindowKey,
  },
]
