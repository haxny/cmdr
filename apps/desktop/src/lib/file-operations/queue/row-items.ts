/**
 * What a queue row says it acts on: "2 folders and 1 file", the names behind
 * that on hover, and the short name of the folder they come from. The backend
 * spells out `OperationItems` once, at registration; this only words it.
 * DETAILS § "Row layout".
 */

import type { OperationItems } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

/** "2 folders and 1 file" where the starter said which is which, else "3 items". */
export function itemsPhrase(items: OperationItems): string {
  const { kinds } = items
  if (kinds === null) {
    return tString('queue.row.itemCount', { countText: formatInteger(items.count), count: items.count })
  }
  const folders = tString('queue.row.folderCount', { countText: formatInteger(kinds.folders), count: kinds.folders })
  const files = tString('queue.row.fileCount', { countText: formatInteger(kinds.files), count: kinds.files })
  if (kinds.files === 0) return folders
  if (kinds.folders === 0) return files
  return tString('queue.row.foldersAndFiles', { folders, files })
}

/** The first names, one per line, then how many more there are. */
export function itemsTooltip(items: OperationItems): string {
  const more = items.count - items.names.length
  if (more <= 0) return items.names.join('\n')
  return [...items.names, tString('queue.row.itemsMore', { countText: formatInteger(more), count: more })].join('\n')
}

/** The last folder of a display path (`~/Dropbox` → `Dropbox`), or the whole of
 *  one with nothing past its root (`~`, `/`, a volume's own name). */
export function locationName(location: string): string {
  const trimmed = location.replace(/\/+$/, '')
  const slash = trimmed.lastIndexOf('/')
  return slash >= 0 ? trimmed.slice(slash + 1) : location
}
