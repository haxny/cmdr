/**
 * The Tags window's opener: one window per Multi-Rename session the window holds, labelled `tags-<sessionId>`, on the
 * `/tags` route. Only the main window opens it (`open-tags.ts`); the window itself gets no window-creation perms.
 * A second open of the same session raises the window it has (`raiseChildWindow` also restores a minimized one).
 * Placed like the Keyboard shortcuts window: where the last Tags window was this session, else centered on main.
 */

import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { raiseChildWindow } from '#lib/tauri-commands/index.js'
import { decorateChildWindowTitle, isE2eRun, orderChildWindowToBackInE2e } from '#lib/app-mode.js'
import { readMainRect, readMonitors, readSavedRect, resolveChildPosition } from '#lib/window-positioning.js'

const log = getAppLogger('tagEditor')

/** The key the windows share for their remembered place (`trackOwnRect`), so the next one opens where the last was. */
export const TAGS_WINDOW_RECT_KEY = 'tags'

const WIDTH = 1100
const HEIGHT = 760
const MIN_WIDTH = 720
const MIN_HEIGHT = 480

export function tagsWindowLabel(sessionId: string): string {
  return `tags-${sessionId}`
}

export function tagsWindowUrl(sessionId: string, folderName: string): string {
  return `/tags?session=${encodeURIComponent(sessionId)}&folder=${encodeURIComponent(folderName)}`
}

/** Opens the Tags window of `sessionId` (a session already held for it), or raises the one it has. */
export async function openTagsWindow(sessionId: string, folderName: string): Promise<void> {
  const label = tagsWindowLabel(sessionId)
  // E2E runs open windows without stealing OS focus (the plugin drives them over a socket).
  const isE2e = isE2eRun()
  if (await WebviewWindow.getByLabel(label)) {
    if (!isE2e) await raiseChildWindow(label)
    return
  }
  const size = { width: WIDTH, height: HEIGHT }
  const [main, monitors, saved] = await Promise.all([
    readMainRect(),
    readMonitors(),
    readSavedRect(TAGS_WINDOW_RECT_KEY),
  ])
  const rect = main ? resolveChildPosition({ size, main, monitors, saved }) : null
  log.debug('Opening a Tags window')
  const win = new WebviewWindow(label, {
    url: tagsWindowUrl(sessionId, folderName),
    title: decorateChildWindowTitle(tString('multiRename.tags.window.title', { folder: folderName })),
    width: rect?.width ?? WIDTH,
    height: rect?.height ?? HEIGHT,
    minWidth: MIN_WIDTH,
    minHeight: MIN_HEIGHT,
    ...(rect ? { x: rect.x, y: rect.y } : { center: true }),
    resizable: true,
    minimizable: true,
    maximizable: true,
    closable: true,
    focus: !isE2e,
    // The overlay title bar of the Settings and Keyboard shortcuts windows: the page paints its own title row.
    titleBarStyle: 'overlay',
    hiddenTitle: true,
    trafficLightPosition: new LogicalPosition(14, 18),
  })
  void orderChildWindowToBackInE2e(win)
}
