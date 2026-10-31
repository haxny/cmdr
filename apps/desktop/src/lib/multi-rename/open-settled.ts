/**
 * Opens a Multi-rename session across a folder that's still changing. `selectionChanged` means
 * the backend's rows moved past the pane's for a moment, so this waits it out the way
 * `readPressedRows` does for F5 and F6, instead of turning the press into a toast.
 */

import { PRESS_RETRY_MS, ROW_SETTLE_BUDGET_MS } from '#lib/file-explorer/pane/pressed-rows.js'
import type { MultiRenameSelection } from '#lib/file-explorer/pane/pane-commands.js'
import type { MultiRenameResult } from '#lib/tauri-commands/index.js'

/**
 * Re-reads the target for each attempt, since the pane carries its selection through every diff.
 * `null` when there's nothing to open, which the target's resolver has already said. A different
 * listing ends the wait: the press was about the folder it left.
 */
export async function openSettled<T>(
  resolveTarget: () => MultiRenameSelection | null,
  open: (target: MultiRenameSelection) => Promise<MultiRenameResult<T>>,
): Promise<MultiRenameResult<T> | null> {
  const deadline = Date.now() + ROW_SETTLE_BUDGET_MS
  let target = resolveTarget()
  if (!target) return null
  const listingId = target.listingId
  for (;;) {
    const opened = await open(target)
    const stale = !opened.ok && opened.error.type === 'selectionChanged'
    if (!stale || Date.now() + PRESS_RETRY_MS > deadline) return opened
    await new Promise((resolve) => setTimeout(resolve, PRESS_RETRY_MS))
    target = resolveTarget()
    if (!target) return null
    if (target.listingId !== listingId) return opened
  }
}
