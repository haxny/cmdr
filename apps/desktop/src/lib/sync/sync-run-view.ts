/**
 * Where the run's result sends the person: "Show in Cmdr" opens a versions folder in the pane whose root is on its
 * volume (the right, the usual target, when both or neither are), and "Show in grid" shows every row so the rows
 * that didn't run can't hide behind a filter.
 */
import type { ShowFilters } from '#lib/tauri-commands/sync.js'
import type { SyncRoots } from './pane-roots'

export interface ShowFolder {
  pane: 'left' | 'right'
  volumeId: string
  path: string
}

export function versionsFolderPane(roots: SyncRoots, volumeId: string): 'left' | 'right' {
  return roots.left.root.volumeId === volumeId && roots.right.root.volumeId !== volumeId ? 'left' : 'right'
}

export function showInGridFilters(): ShowFilters {
  return {
    toRight: true,
    equal: true,
    different: true,
    toLeft: true,
    duplicates: true,
    singles: true,
    unknown: true,
    warningsOnly: false,
  }
}
