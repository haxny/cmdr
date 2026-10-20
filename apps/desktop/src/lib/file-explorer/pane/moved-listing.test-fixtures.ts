/**
 * A backend listing a watcher insert moved on from what the pane applied, for
 * the specs pinning that delete, the clipboard, and drag refuse old row numbers.
 *
 * The pane applied revision 0, `[alpha, charlie]`, and selected `charlie` on
 * backend row 1. Then `bravo` landed above it: the backend is at revision 1,
 * `[alpha, bravo, charlie]`, so row 1 is now `bravo`, a file nobody selected.
 *
 * It answers the revision-checked reads (`getSelectionSnapshot` /
 * `getSelectionEntries`) the way the backend does: revision 0 is refused as
 * `changed`, so a caller that resolves the press against what the pane applied
 * acts on nothing rather than on `bravo`.
 *
 * Imports only types and the typed refusal, so a spec's `vi.mock` factory can
 * reach it through a lazy `await import`.
 */
import { ListingLookupFailure } from '#lib/tauri-commands/listing-gone.js'
import type { FileEntry } from '../types'

export const MOVED_LISTING_ID = 'listing-moved'
const DIR = '/moved'

function entry(name: string): FileEntry {
  return {
    name,
    path: `${DIR}/${name}`,
    isDirectory: false,
    isSymlink: false,
    size: 1,
    iconId: 'ext:txt',
  } as FileEntry
}

/** The backend's rows now, at `BACKEND_SEQUENCE`. */
const rows = [entry('alpha.txt'), entry('bravo.txt'), entry('charlie.txt')]

/** The revision the pane applied its selection in. */
export const APPLIED_SEQUENCE = 0
/** The revision the watcher insert moved the backend to. */
export const BACKEND_SEQUENCE = 1
/** The backend row the pane selected `charlie.txt` on, at `APPLIED_SEQUENCE`. */
export const SELECTED_ROW = 1
export const SELECTED_PATH = `${DIR}/charlie.txt`

function refuseUnlessCurrent(listingId: string, expectedSequence: number): void {
  if (listingId !== MOVED_LISTING_ID) throw new ListingLookupFailure({ type: 'gone', listingId })
  if (expectedSequence !== BACKEND_SEQUENCE) throw new ListingLookupFailure({ type: 'changed', listingId })
}

/** The revision-checked entries read (`getSelectionEntries`). */
export function selectionEntries(
  listingId: string,
  _includeHidden: boolean,
  indices: number[],
  expectedSequence: number,
): Promise<FileEntry[]> {
  try {
    refuseUnlessCurrent(listingId, expectedSequence)
  } catch (error) {
    return Promise.reject(error)
  }
  return Promise.resolve(indices.map((index) => rows[index]))
}

/** The revision-checked paths read (`getSelectionSnapshot`). */
export async function selectionSnapshot(
  listingId: string,
  includeHidden: boolean,
  indices: number[],
  expectedSequence: number,
): Promise<{ paths: string[]; fileCount: number; folderCount: number }> {
  const entries = await selectionEntries(listingId, includeHidden, indices, expectedSequence)
  return { paths: entries.map((e) => e.path), fileCount: entries.length, folderCount: 0 }
}
