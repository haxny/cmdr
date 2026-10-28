/**
 * The Tags window's own keys (`Tags window`, fork, behind `multiRename.tagEditor`). Pure data; the window's keydown
 * asks `eventMatchesCommand` for each (`multi-rename/tag-editor/TagsWindow.svelte`). Every key is fixed (`fixedKey`):
 * Settings doesn't rebind them; the window still reads the shortcuts store, so lifting that is a flag per entry. See `../command-registry.ts` for how the
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
  // Asks "Discard tag changes?" first when edits are unwritten. ⌘W also reaches the window as the native Close tab
  // item, which closes the focused child window through its close request: the same guard.
  { id: 'tags.close', nameKey: 'commands.tagsClose.label', shortcuts: ['Escape', '⌘W'], ...tagsWindowKey },
  // The two panes (Album | Tracks) switch like the main window's file panes (`pane.switch`). Answered by the pane
  // that has focus (`AlbumPane` / `TracksPane`), so Tab stays the browser's anywhere else in the window.
  { id: 'tags.switchPane', nameKey: 'commands.tagsSwitchPane.label', shortcuts: ['Tab'], ...tagsWindowKey },
  // Edits the cursor row's value in place, as F2 renames a file: the Album pane's value, the Tracks pane's Title.
  { id: 'tags.edit', nameKey: 'commands.tagsEdit.label', shortcuts: ['F2', 'Enter'], ...tagsWindowKey },
  { id: 'tags.editNumber', nameKey: 'commands.tagsEditNumber.label', shortcuts: ['⇧F2'], ...tagsWindowKey },
  // Plays or stops the Tracks pane's cursor row. Plain Space selects (`selection.toggle`), as in a file pane, so the
  // second key is ⌥Space (macOS sends a non-breaking space for it, which `TracksPane` claims when it acts).
  { id: 'tags.play', nameKey: 'commands.tagsPlay.label', shortcuts: ['F3', '⌥Space'], ...tagsWindowKey },
  // The function-key bar's tools (`TagsFunctionKeyBar`): each opens its dialog over the panes and runs on the Tracks
  // pane's selection. The window's keydown answers them (`TagsWindow`), never while a value is typed.
  { id: 'tags.openInPlayer', nameKey: 'commands.tagsOpenInPlayer.label', shortcuts: ['⇧F3'], ...tagsWindowKey },
  { id: 'tags.replace', nameKey: 'commands.tagsReplace.label', shortcuts: ['F4'], ...tagsWindowKey },
  { id: 'tags.case', nameKey: 'commands.tagsCase.label', shortcuts: ['⇧F4'], ...tagsWindowKey },
  { id: 'tags.format', nameKey: 'commands.tagsFormat.label', shortcuts: ['F5'], ...tagsWindowKey },
  { id: 'tags.cleanUp', nameKey: 'commands.tagsCleanUp.label', shortcuts: ['⇧F5'], ...tagsWindowKey },
  { id: 'tags.fromName', nameKey: 'commands.tagsFromName.label', shortcuts: ['F6'], ...tagsWindowKey },
  { id: 'tags.number', nameKey: 'commands.tagsNumber.label', shortcuts: ['F7'], ...tagsWindowKey },
  { id: 'tags.clearField', nameKey: 'commands.tagsClearField.label', shortcuts: ['F8'], ...tagsWindowKey },
  { id: 'tags.removeExtras', nameKey: 'commands.tagsRemoveExtras.label', shortcuts: ['⇧F8'], ...tagsWindowKey },
  // Lists the changes in a dialog, then writes on confirm. From a value being typed it sends that value first.
  { id: 'tags.write', nameKey: 'commands.tagsWrite.label', shortcuts: ['⌘S'], ...tagsWindowKey },
  // Undoes the last write (asks first). While a value is typed ⌘Z stays the box's own text undo.
  { id: 'tags.undo', nameKey: 'commands.tagsUndo.label', shortcuts: ['⌘Z'], ...tagsWindowKey },
]
