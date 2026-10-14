/**
 * Keeps a secondary window's OS title in the app's language.
 *
 * A window title is a native `setTitle` write, not markup, so a language change
 * re-renders everything in the window except the title bar and the Window menu
 * entry. This re-writes it whenever the copy it reads moves: `tString()` reads
 * the locale-version rune, so the effect re-runs on every `setLocale()`.
 */
import { getCurrentWindow } from '@tauri-apps/api/window'

import { decorateChildWindowTitle, initAppMode } from '#lib/app-mode.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('windowTitle')

/**
 * Writes `getTitle()` as this window's title now and again on every language
 * change. Waits for the app mode first, so the E2E and capture title markers
 * survive the write. Needs `core:window:allow-set-title` in the window's
 * capability file.
 *
 * @param getTitle builds the undecorated title (read reactive state inside it),
 *   or `null` to leave the title alone for now (a viewer before its file loads)
 * @param writeTitle test seam; defaults to this window's `setTitle`
 * @returns a teardown that stops following
 */
export function followWindowTitle(
  getTitle: () => string | null,
  writeTitle: (title: string) => Promise<void> = (title) => getCurrentWindow().setTitle(title),
): () => void {
  let stopEffect: (() => void) | undefined
  let stopped = false
  let lastWritten: string | undefined

  void initAppMode().then(() => {
    if (stopped) return
    stopEffect = $effect.root(() => {
      $effect(() => {
        const undecorated = getTitle()
        if (undecorated === null) return
        const title = decorateChildWindowTitle(undecorated)
        // A region change bumps the rune with the copy unchanged.
        if (title === lastWritten) return
        lastWritten = title
        writeTitle(title).catch((error: unknown) => {
          log.warn('Couldn’t update the window title: {error}', { error: String(error) })
        })
      })
    })
  })

  return () => {
    stopped = true
    stopEffect?.()
  }
}
