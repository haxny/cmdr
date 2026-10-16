/**
 * The searchable non-setting rows of `UpdatesSection.svelte`.
 *
 * Declaration only: it makes the row findable and lets its card know to show,
 * and never decides what renders. The section hand-renders the markup and gates
 * it on `shouldShow(id)` with this id.
 */

import type { SearchableRow } from '../types'

export const updatesRows: SearchableRow[] = [
  {
    // The "Check for updates" button leading the Updates card.
    id: 'row:updates.checkForUpdates',
    section: ['Updates & privacy'],
    labelKey: 'settings.updates.checkForUpdates',
    cardKey: 'settings.updates.card.updates',
    keywords: ['update', 'upgrade', 'check', 'version', 'new version'],
  },
  {
    // The "Delete all Cmdr data" button, alone in its card.
    id: 'row:updates.deleteAllData',
    section: ['Updates & privacy'],
    labelKey: 'settings.dataWipe.button',
    cardKey: 'settings.dataWipe.card',
    keywords: ['delete', 'erase', 'remove', 'wipe', 'reset', 'uninstall', 'privacy', 'data', 'forget', 'gdpr'],
  },
]
