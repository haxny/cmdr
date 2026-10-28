/**
 * The Tags window's discard guard: closing the window (Escape, the red button) while tag edits are unwritten asks
 * "Discard tag changes?" first, so nothing is lost silently. `TagsWindow.svelte` holds one line of it.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { confirmDialog } from '#lib/utils/confirm-dialog.js'
import type { TagEditorState } from './tag-editor-state.svelte'

const log = getAppLogger('multiRename')

export interface DiscardGuard {
  /** There are tag edits not yet written to the files, or one still on its way. */
  readonly dirty: boolean
  /** Resolves true when the window may go on to close: nothing to lose, or the user chose Discard. */
  confirm: () => Promise<boolean>
  /** Closes right away when nothing is unwritten; otherwise asks first (one question at a time). */
  close: () => void
}

/** With `tags` `null` the guard never asks and `close` is `onClose`. */
export function createDiscardGuard(tags: TagEditorState | null, onClose: () => void): DiscardGuard {
  let asking = false

  async function confirm(): Promise<boolean> {
    // A cell that just lost focus has an edit on its way; the question is about what it leaves. A call that never
    // answers (a stalled mount) can't hold the window: after the wait it asks anyway.
    const answered = (await tags?.settled()) ?? true
    if (answered && tags?.view?.dirty !== true) return true
    return confirmDialog(
      tString('multiRename.tags.discard.message'),
      tString('multiRename.tags.discard.title'),
      tString('multiRename.tags.discard.confirm'),
    )
  }

  const guard: DiscardGuard = {
    get dirty() {
      return tags !== null && (tags.busy || tags.view?.dirty === true)
    },
    confirm,
    close() {
      if (!guard.dirty) {
        onClose()
        return
      }
      if (asking) return
      asking = true
      void confirm()
        .then((discard) => {
          if (discard) onClose()
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
