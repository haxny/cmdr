/**
 * The Tags tab's discard guard: closing the sheet, or pressing Rename, while tag edits are unwritten asks
 * "Discard tag changes?" first, so nothing is lost silently. The dialog holds one line of it; the logic lives here.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { confirmDialog } from '#lib/utils/confirm-dialog.js'
import type { TagEditorState } from './tag-editor-state.svelte'

const log = getAppLogger('multiRename')

export interface DiscardGuard {
  /** There are tag edits not yet written to the files. */
  readonly dirty: boolean
  /** Resolves true when the sheet may go on to close or rename: nothing to lose, or the user chose Discard. */
  confirm(): Promise<boolean>
  /** Closes right away when nothing is unwritten; otherwise asks first (one question at a time). */
  close(): void
}

/** `tags` is `null` while the Tags tab is off: the guard then never asks and `close` is `onClose`. */
export function createDiscardGuard(tags: TagEditorState | null, onClose: () => void): DiscardGuard {
  let asking = false

  async function confirm(): Promise<boolean> {
    if (tags?.view?.dirty !== true) return true
    return confirmDialog(
      tString('multiRename.tags.discard.message'),
      tString('multiRename.tags.discard.title'),
      tString('multiRename.tags.discard.confirm'),
    )
  }

  return {
    get dirty() {
      return tags?.view?.dirty === true
    },
    confirm,
    close() {
      if (tags?.view?.dirty !== true) {
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
          // A question that couldn't be asked keeps the sheet open: nothing is lost.
          log.warn("couldn't ask about unwritten tag changes: {reason}", { reason: e })
        })
        .finally(() => {
          asking = false
        })
    },
  }
}
