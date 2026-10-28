/**
 * The Tags window's answer to a `CloseRequested`: the red button (`routes/tags/+page.svelte` registers it). While the
 * body is live it holds the close and asks the discard question (`requestClose`), as Escape does; otherwise it lets
 * the close through.
 *
 * ❗ It never throws, and when in doubt it lets the close go: Tauri hands the close to this listener and waits, so a
 * listener that throws or vetoes without asking strands the window alive (hidden, once Rust has hidden it). The
 * body is `null` once Svelte has unmounted it (`bind:this`), `undefined` before it mounts.
 * Escape/⌘W never reach here: `closeSelfWindow` destroys the window outright (`commands/child_window_state.rs`).
 */
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('tagEditor')

export interface ClosableBody {
  requestClose: () => void
  isEnding: () => boolean
}

export function onTagsCloseRequested(
  body: ClosableBody | null | undefined,
  event: { preventDefault: () => void },
): void {
  if (body == null) return
  let ending: boolean
  try {
    ending = body.isEnding()
  } catch (error) {
    log.warn('Tags window: the body could not answer a close request, so the window closes: {error}', { error })
    return
  }
  if (ending) return
  event.preventDefault()
  try {
    body.requestClose()
  } catch (error) {
    // The window stays up and visible, so Escape still closes it.
    log.warn('Tags window: the close question did not open: {error}', { error })
  }
}
