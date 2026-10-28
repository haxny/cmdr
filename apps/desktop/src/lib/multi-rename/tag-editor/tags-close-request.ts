/**
 * The Tags window's answer to a `CloseRequested`: the red button (`routes/tags/+page.svelte` registers it). While the
 * body is live it holds the close and asks the discard question (`requestClose`), as Escape does; otherwise it lets
 * the close through.
 *
 * ❗ It never throws, and when in doubt it lets the close go: Tauri hands the close to this listener and waits, so a
 * listener that throws or vetoes without asking strands the window alive (hidden, once Rust has hidden it). The
 * body is `null` once Svelte has unmounted it (`bind:this`), `undefined` before it mounts. With no body nothing can
 * ask, so a red button in that gap closes unasked; the gap is short, since the error boundary remounts the body on
 * the next tick (`closeAfterBodyFailed` keeps the window for it). A body that throws on every remount ends the
 * reloads: the window then shows a worded fallback whose Close, and the red button, go through `closeBrokenWindow`.
 * Escape/⌘W never reach here: `closeSelfWindow` destroys the window outright (`commands/child_window_state.rs`).
 */
import { getAppLogger } from '#lib/logging/logger.js'
import { closeSelfWindow } from '#lib/child-window-close.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { confirmDialog } from '#lib/utils/confirm-dialog.js'
import { closeMultiRename, closeTagEditor, releaseTagsSession, tagsWindowMayClose } from '#lib/tauri-commands/index.js'

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

/**
 * The window's error boundary closes the broken body here, and it may close the WINDOW only when nothing would be
 * lost. The edits live in the backend session, and a closed window ends it (`Destroyed`), so a body that threw while
 * edits were unwritten, or mid-write, keeps the window: the boundary remounts the body, which reads the same session
 * back with its edits. When the backend can't say, the window stays too; the quit gate still asks.
 */
export async function closeAfterBodyFailed(sessionId: string): Promise<void> {
  let mayClose: boolean
  try {
    mayClose = await tagsWindowMayClose(sessionId)
  } catch (error) {
    log.warn('Tags window: no answer on unwritten edits, so the window stays: {error}', { error })
    return
  }
  if (!mayClose) {
    log.info('Tags window: its body broke with edits unwritten or a write running, so it reloads in place')
    return
  }
  await closeSelfWindow()
}

/**
 * Closes a Tags window whose body stopped reloading (its fallback's Close, and the red button then): with unwritten
 * edits or a write running (or when the backend can't say) it asks the discard question ⌘W asks, then releases the
 * hold, ends the tag and Multi-Rename sessions, and closes the window. A write still running finishes in the backend,
 * which records its undo; only the unwritten edits go. Resolves whether the window is closing.
 */
export async function closeBrokenWindow(sessionId: string): Promise<boolean> {
  let mayClose = false
  try {
    mayClose = await tagsWindowMayClose(sessionId)
  } catch (error) {
    log.warn('Tags window: no answer on unwritten edits, so it asks first: {error}', { error })
  }
  if (!mayClose) {
    const discard = await confirmDialog(
      tString('multiRename.tags.discard.message'),
      tString('multiRename.tags.discard.title'),
      tString('multiRename.tags.discard.confirm'),
    )
    if (!discard) return false
  }
  try {
    await releaseTagsSession(sessionId)
    await closeTagEditor(sessionId)
    await closeMultiRename(sessionId)
  } catch (error) {
    log.warn("Tags window: couldn't end its session before closing: {error}", { error })
  } finally {
    await closeSelfWindow()
  }
  return true
}
