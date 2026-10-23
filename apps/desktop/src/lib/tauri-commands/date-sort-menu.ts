import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type DateSortMenuPick } from '#lib/ipc/bindings.js'
import type { SortColumn } from '#lib/file-explorer/types.js'
import type { MenuAnchor } from './file-actions'
import { throwIpcError } from './ipc-types'

/**
 * Shows a file list header's native Sort by popup over `pane`: Date modified, Date created,
 * and Date added, `requested` checked. `anchor` is where the chevron opens it (viewport CSS
 * px); `null` opens it at the pointer, for a right-click. Resolves once the menu closes
 * (`popup()` blocks in Rust until then); the pick arrives later as `onDateSortMenuPick`.
 */
export async function showDateSortMenu(
  pane: 'left' | 'right',
  requested: SortColumn | null,
  anchor: MenuAnchor | null,
): Promise<void> {
  const res = await commands.showDateSortMenu(pane, requested, anchor)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * A date picked from a header's Sort by popup, with the pane the menu opened over. Emitted
 * by `on_menu_event` after `popup()` returns, since muda queues the `MenuEvent`.
 */
export function onDateSortMenuPick(handler: (pick: DateSortMenuPick) => void): Promise<UnlistenFn> {
  return events.dateSortMenuPick.listen((event) => {
    handler(event.payload)
  })
}
