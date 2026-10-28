/**
 * A picture file dropped on the cover section. Under Tauri's `dragDropEnabled` a file drag (from Finder or a Cmdr pane)
 * reaches the page as the webview's native drag-drop event, ❌ never as an HTML5 `drop` with a `DataTransfer`, so the
 * section subscribes to that event and hit-tests its own rect, as the Ask Cmdr composer does (`ask-cmdr-drop.ts`).
 * The Tags window is its own webview, so the main window's self-drag identity isn't here: the payload paths are used,
 * and the backend opens one only as a regular file (`set_cover_from_path`). Listening needs only `core:event:default`.
 */

import { toViewportPosition } from '#lib/file-explorer/drag/drag-position.js'

/** The native drag-drop payload (the Tauri webview's shape; only the fields read here). */
export type CoverDropEvent =
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' }

/** Subscribes `handler` to the window's file drags; resolves to the unsubscribe. */
export type CoverDropSubscriber = (handler: (event: CoverDropEvent) => void) => Promise<() => void>

/** Whether `position` (a drag event's, DevTools offset corrected) is inside `rect`. No rect is never a hit. */
export function isOver(position: { x: number; y: number }, rect: DOMRect | null): boolean {
  if (!rect) return false
  const at = toViewportPosition(position)
  return at.x >= rect.left && at.x <= rect.right && at.y >= rect.top && at.y <= rect.bottom
}

/** The webview's native drag-drop events. Outside a Tauri webview (unit tests) it subscribes to nothing. */
export const subscribeCoverDrop: CoverDropSubscriber = async (handler) => {
  try {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview')
    return await getCurrentWebview().onDragDropEvent((event) => {
      handler(event.payload)
    })
  } catch {
    return () => {}
  }
}
