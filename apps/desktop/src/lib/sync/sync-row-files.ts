/**
 * F3 / F4 / ⌥Space on a grid row: the file on one side as a path on its volume. A row carries only its name, so the
 * folder comes from the header row above it (root rows have none; they come first).
 *
 * Gap: the header names the folder in the LEFT spelling. On a right side whose folder differs only in case or
 * Unicode form, on a case-sensitive volume, the right path can miss; viewing it then says it isn't there.
 */
import type { Side, ViewRowWire } from '#lib/tauri-commands/index.js'
import type { SyncRoots } from './pane-roots'
import type { SyncItemRow } from './row-label'

export interface SideFile {
  volumeId: string
  path: string
}

export type SideFileRefusal = { type: 'noFile' } | { type: 'notAFile' }

/** What `rowAt` gives: a view row, loading its page when needed. */
export type RowAt = (viewIndex: number) => Promise<ViewRowWire | undefined>

/** The folder a view row sits in, from the nearest header above it; `''` for the root's own rows. */
export async function relDirOf(rowAt: RowAt, viewIndex: number): Promise<string> {
  for (let index = viewIndex - 1; index >= 0; index--) {
    const row = await rowAt(index)
    if (row?.type === 'folder') return row.relDir
  }
  return ''
}

export function joinPath(root: string, relDir: string, name: string): string {
  const base = root.endsWith('/') ? root.slice(0, -1) : root
  const below = [relDir, name].filter((part) => part !== '').join('/')
  return `${base}/${below}`
}

/** The row's file on `side`, or why there's none to open. */
export async function sideFileOf(
  rowAt: RowAt,
  roots: SyncRoots,
  at: { viewIndex: number; row: SyncItemRow; side: Side },
): Promise<SideFile | SideFileRefusal> {
  const { viewIndex, row, side } = at
  const name = side === 'left' ? row.leftName : row.rightName
  const kind = side === 'left' ? row.leftKind : row.rightKind
  if (name === null) return { type: 'noFile' }
  if (kind === 'folder') return { type: 'notAFile' }
  const relDir = await relDirOf(rowAt, viewIndex)
  const root = roots[side].root
  return { volumeId: root.volumeId, path: joinPath(root.path, relDir, name) }
}

export function isSideFile(answer: SideFile | SideFileRefusal): answer is SideFile {
  return 'path' in answer
}
