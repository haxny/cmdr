/**
 * The two sides the dialog opens on: each pane's volume and folder, plus its selection for "Only selected" (TC's
 * "Only selected (in main window)"). Row numbers go out in the backend's own numbering (no `..` row) with the
 * listing sequence they were read at, so the backend can refuse a selection that moved on (`staleSelection`).
 */
import type { FilePaneAPI } from '#lib/file-explorer/pane/types.js'
import { toBackendIndices } from '#lib/file-operations/transfer/transfer-dialog-utils.js'
import type { SideRequest } from '#lib/tauri-commands/index.js'

export interface SyncRoots {
  left: SideRequest
  right: SideRequest
}

export interface PaneRootsDeps {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
}

function sideOf(pane: FilePaneAPI, onlySelected: boolean): SideRequest | null {
  const listingId = pane.getListingId()
  if (listingId === '') return null
  const root = { volumeId: pane.getVolumeId(), path: pane.getCurrentPath() }
  const sorted = [...pane.getSelectedIndices()].sort((a, b) => a - b)
  const indices = onlySelected ? toBackendIndices(sorted, pane.hasParentEntry()) : []
  if (indices.length === 0) return { root, selection: null }
  return { root, selection: { listingId, indices, expectedSequence: pane.getLastSequence() } }
}

/**
 * Both panes as roots, selections taken only when `onlySelected` (and only from a pane that has one). `null` when a
 * pane shows no folder listing (the network hub, search results).
 */
export function syncRootsOf(deps: PaneRootsDeps, onlySelected: boolean): SyncRoots | null {
  const leftPane = deps.getPaneRef('left')
  const rightPane = deps.getPaneRef('right')
  if (!leftPane || !rightPane) return null
  const left = sideOf(leftPane, onlySelected)
  const right = sideOf(rightPane, onlySelected)
  return left && right ? { left, right } : null
}

/** The same roots without the selections, for a compare with "Only selected" off. */
export function withoutSelections(roots: SyncRoots): SyncRoots {
  return { left: { ...roots.left, selection: null }, right: { ...roots.right, selection: null } }
}
