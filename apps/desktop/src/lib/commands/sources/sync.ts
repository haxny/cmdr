/**
 * Synchronize folders (⇧F12), Total Commander's Synchronize dirs: the dialog's opener. TC ships no key for it; ⇧F12
 * is the one its users bind, and it pairs with ⇧F2 (Compare folders). The dialog's own keys live in
 * `#lib/sync/sync-keys.ts`. See `../command-registry.ts` for how the scope arrays are concatenated.
 */
import { getBadgeStatus } from '#lib/feature-status.js'
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const syncCommands: CommandSource[] = [
  {
    id: 'sync.openDialog',
    nameKey: 'commands.syncOpenDialog.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⇧F12'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.syncOpenDialog.description',
    status: getBadgeStatus('synchronize-folders'),
    keywords: ['compare folders', 'mirror folders', 'backup folders', 'sync directories'],
  },
  {
    // The dialog on the panes' folders with its Profiles menu open. No key: inside the dialog F2 opens the menu.
    id: 'sync.openProfiles',
    nameKey: 'commands.syncOpenProfiles.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    keywords: ['sync profile', 'synclet', 'saved folder pair'],
  },
]
