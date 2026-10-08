import { getFileBeside } from '$lib/tauri-commands'
import type { FileEntry } from '../types'
import type { RenameTarget } from '../rename/rename-state.svelte'
import type { RenameStepDirection } from '../rename/rename-step'

/** What a chained rename step reads to find the row beside the editor. */
export interface RenameNeighbourDeps {
  /** The file the editor is open on, `undefined` when no editor is. */
  getTarget: () => RenameTarget | undefined
  getListingId: () => string
  getIncludeHidden: () => boolean
  getHasParent: () => boolean
  /** A row read straight out of the loaded window; `undefined` when it isn't loaded. */
  getEntryAt: (index: number) => FileEntry | undefined
  /** The row a path occupies in the loaded window; `undefined` when it isn't loaded. */
  indexOfEntry: (path: string) => number | undefined
}

/**
 * Finds the row beside the one the rename editor is drawn on, for a chained
 * ArrowUp / ArrowDown step.
 *
 * Every answer is anchored on the editor's own file, ❌ never on the cursor's
 * index. A pane holds three listings that disagree for a beat each time a
 * chain's own rename lands: the backend mutates its listing the moment a rename
 * does, the cursor is reconciled when the `directory-diff` for it arrives 50 ms
 * later, and the window those rows are READ from is refetched on a throttle
 * after that. An index means a different row in each of them, so a step that
 * carries one across skips a row, or reopens the editor on the file whose rename
 * it just sent (which the diff for that rename then closes, ending the chain
 * with nothing said). The editor mounts BY PATH, so its file is the one thing
 * all three agree on.
 */
export function createRenameNeighbour(deps: RenameNeighbourDeps) {
  return {
    /**
     * The neighbour read out of the loaded window the user is looking at, so the
     * chain lands where they were looking. It costs a `findIndex` over the loaded
     * rows and no round trip.
     */
    inLoadedWindow(direction: RenameStepDirection): FileEntry | undefined {
      const targetPath = deps.getTarget()?.path
      if (targetPath === undefined) return undefined
      const editorRow = deps.indexOfEntry(targetPath)
      if (editorRow === undefined) return undefined
      const beside = direction === 'down' ? editorRow + 1 : editorRow - 1
      // `..` is nothing to rename, and the window would hand it over happily.
      if (beside < (deps.getHasParent() ? 1 : 0)) return undefined
      return deps.getEntryAt(beside)
    },

    /**
     * The same question, asked of the backend when the window can't answer for
     * the row (a chain that has outrun the pane's prefetch).
     *
     * In ONE call: resolving the anchor's index and reading beside it separately
     * lets a rename land in between and move the row out from under the index.
     */
    async fetch(direction: RenameStepDirection): Promise<FileEntry | undefined> {
      const originalName = deps.getTarget()?.originalName
      if (originalName === undefined) return undefined
      try {
        const side = direction === 'down' ? 'next' : 'previous'
        return (await getFileBeside(deps.getListingId(), originalName, side, deps.getIncludeHidden())) ?? undefined
      } catch {
        return undefined
      }
    },
  }
}
