/**
 * The Tags window's discard guard: closing the window (Escape, ⌘W, the red button) while tag edits are unwritten asks
 * "Discard tag changes?" first, so nothing is lost silently. While a write or an undo is on its way it never asks and
 * never closes: `onWriteRunning` says to wait, and once the write is done its outcome shows and closing works again.
 * A read stuck on a stalled share is no edit, so it doesn't make the window ask. `TagsWindow.svelte` holds one.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { confirmDialog } from '#lib/utils/confirm-dialog.js'
import type { TagEditorState } from './tag-editor-state.svelte'

const log = getAppLogger('multiRename')

export interface DiscardGuard {
  /** There are tag edits not yet written to the files, one still on its way, or a write running. */
  readonly dirty: boolean
  /** Resolves true when the window may go on to close: nothing to lose, or the user chose Discard. */
  confirm: () => Promise<boolean>
  /** Closes right away when nothing is unwritten; otherwise asks first (one question at a time). */
  close: () => void
}

/** With `tags` `null` the guard never asks and `close` is `onClose`. */
export function createDiscardGuard(
  tags: TagEditorState | null,
  onClose: () => void,
  onWriteRunning: () => void = () => {},
): DiscardGuard {
  let asking = false

  async function confirm(): Promise<boolean> {
    // A cell that just lost focus has an edit on its way; the question is about what it leaves. A call that never
    // answers (a stalled mount) can't hold the window: after the wait an edit still out asks, a read doesn't.
    const answered = (await tags?.settled()) ?? true
    const editStillOut = !answered && tags?.editing === true
    if (!editStillOut && tags?.view?.dirty !== true) return true
    return confirmDialog(
      tString('multiRename.tags.discard.message'),
      tString('multiRename.tags.discard.title'),
      tString('multiRename.tags.discard.confirm'),
    )
  }

  /** Closes, unless a write started meanwhile: then it says to wait, as a close during a write does. */
  function closeUnlessWriting(): void {
    if (tags?.writing === true) onWriteRunning()
    else onClose()
  }

  const guard: DiscardGuard = {
    get dirty() {
      return tags !== null && (tags.writing || tags.editing || tags.view?.dirty === true)
    },
    confirm,
    close() {
      if (tags?.writing === true) {
        onWriteRunning()
        return
      }
      if (!guard.dirty) {
        closeUnlessWriting()
        return
      }
      if (asking) return
      asking = true
      void confirm()
        .then((discard) => {
          if (discard) closeUnlessWriting()
        })
        .catch((e: unknown) => {
          // A question that couldn't be asked keeps the window open: nothing is lost.
          log.warn("couldn't ask about unwritten tag changes: {reason}", { reason: e })
        })
        .finally(() => {
          asking = false
        })
    },
  }
  return guard
}
