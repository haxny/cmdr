/**
 * The operation queue window's own keys (`Operation queue window`). Pure data; the window's
 * keydown asks `eventMatchesCommand` for each (`lib/file-operations/queue/queue-keys.ts`).
 * Registered so Settings and the Help window list them. Fixed, because that window has no
 * shortcuts store to read a rebinding from (it holds no `store:` capability). See
 * `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/** Every entry shares these: the window answers them itself, never the dispatch spine. */
const queueWindowKey = {
  scope: 'Operation queue window',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
  fixedKey: true,
} as const

export const queueWindowCommands: CommandSource[] = [
  { id: 'queue.cursorUp', nameKey: 'commands.queueCursorUp.label', shortcuts: ['↑'], ...queueWindowKey },
  { id: 'queue.cursorDown', nameKey: 'commands.queueCursorDown.label', shortcuts: ['↓'], ...queueWindowKey },
  { id: 'queue.cursorFirst', nameKey: 'commands.queueCursorFirst.label', shortcuts: ['Home'], ...queueWindowKey },
  { id: 'queue.cursorLast', nameKey: 'commands.queueCursorLast.label', shortcuts: ['End'], ...queueWindowKey },
  // ⇧ extends the "Cancel selected" set while the cursor moves, as in a macOS table.
  { id: 'queue.extendUp', nameKey: 'commands.queueExtendUp.label', shortcuts: ['⇧↑'], ...queueWindowKey },
  { id: 'queue.extendDown', nameKey: 'commands.queueExtendDown.label', shortcuts: ['⇧↓'], ...queueWindowKey },
  {
    id: 'queue.toggleSelected',
    nameKey: 'commands.queueToggleSelected.label',
    shortcuts: ['Space'],
    ...queueWindowKey,
  },
  // The menu accelerator fires before the page sees ⌘A, so the native menu routes it to
  // the window too (`queue-select-all`); this entry is the keydown road and the listing.
  { id: 'queue.selectAll', nameKey: 'commands.queueSelectAll.label', shortcuts: ['⌘A'], ...queueWindowKey },
  {
    id: 'queue.showInMainWindow',
    nameKey: 'commands.queueShowInMainWindow.label',
    shortcuts: ['Enter'],
    ...queueWindowKey,
  },
  {
    id: 'queue.cancelOrDismiss',
    nameKey: 'commands.queueCancelOrDismiss.label',
    shortcuts: ['Backspace', 'Delete'],
    ...queueWindowKey,
  },
  // ⌥↑ / ⌥↓ is the house reorder key (`lib/ui/menu-navigation.ts`) and the ARIA APG
  // rearrangeable-listbox key. ❌ Not ⇧-arrows (they extend the selection).
  { id: 'queue.moveEarlier', nameKey: 'commands.queueMoveEarlier.label', shortcuts: ['⌥↑'], ...queueWindowKey },
  { id: 'queue.moveLater', nameKey: 'commands.queueMoveLater.label', shortcuts: ['⌥↓'], ...queueWindowKey },
  {
    id: 'queue.nextControl',
    nameKey: 'commands.queueNextControl.label',
    shortcuts: ['→'],
    ...queueWindowKey,
  },
  {
    id: 'queue.previousControl',
    nameKey: 'commands.queuePreviousControl.label',
    shortcuts: ['←'],
    ...queueWindowKey,
  },
  { id: 'queue.close', nameKey: 'commands.queueClose.label', shortcuts: ['Escape'], ...queueWindowKey },
]
