/**
 * F3 / F4 / ⌥Space on a grid row: the file on one side as a path on its volume. Each row carries both sides' paths
 * from the root in their OWN spelling (`leftRelPath`, `rightRelPath`), so a folder that differs in case or Unicode
 * form between the sides still opens the right file.
 */
import type { Side } from '#lib/tauri-commands/sync.js'
import type { SyncRoots } from './pane-roots'
import type { SyncItemRow } from './row-label'

export interface SideFile {
  volumeId: string
  path: string
}

export type SideFileRefusal = { type: 'noFile' } | { type: 'notAFile' }

export function joinPath(root: string, relPath: string): string {
  const base = root.endsWith('/') ? root.slice(0, -1) : root
  return `${base}/${relPath}`
}

/** The row's file on `side`, or why there's none to open. */
export function sideFileOf(roots: SyncRoots, row: SyncItemRow, side: Side): SideFile | SideFileRefusal {
  const relPath = side === 'left' ? row.leftRelPath : row.rightRelPath
  const kind = side === 'left' ? row.leftKind : row.rightKind
  if (relPath === null) return { type: 'noFile' }
  if (kind === 'folder') return { type: 'notAFile' }
  const root = roots[side].root
  return { volumeId: root.volumeId, path: joinPath(root.path, relPath) }
}

export function isSideFile(answer: SideFile | SideFileRefusal): answer is SideFile {
  return 'path' in answer
}
