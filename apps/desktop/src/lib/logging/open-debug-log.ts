import { getDebugLogPath } from '#lib/tauri-commands/index.js'
import { openFileViewer } from '#lib/file-viewer/open-viewer.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { addToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from './logger'

/**
 * Both the Help menu and the palette open the same local, read-only log viewer, tailed so it's live, in the log view:
 * it hides the markers the file keeps for error reports and shows each marked value as itself.
 */
export async function openDebugLog(): Promise<void> {
  try {
    const path = await getDebugLogPath()
    if (path == null) {
      addToast(tString('commands.handler.viewDebugLog.disabled'), { level: 'info' })
      return
    }
    await openFileViewer(path, 'root', { tail: true, logView: true })
  } catch (error) {
    getAppLogger('logging').error('Could not open debug log: {error}', { error: String(error) })
    addToast(tString('commands.handler.viewDebugLog.unavailable'), { level: 'error' })
  }
}
