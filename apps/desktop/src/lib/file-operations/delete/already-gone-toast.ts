/**
 * The note after a delete or trash found some of its items already gone:
 * something else moved or deleted them first, often while a pane still showed
 * them (ERR-X84HN). The backend counts those as done and completes
 * (`crates/cmdr-operations/src/delete/already_gone.rs`), so this is an INFO
 * note, ❌ never an error: nothing went wrong, and the pane refreshes on the
 * completion like after any delete.
 *
 * Self-hides after five seconds and offers "Never show this again", which turns
 * `fileOperations.showAlreadyGoneToast` off; Settings > Advanced turns it back on.
 */

import type { AlreadyGone } from '#lib/ipc/bindings.js'
import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getSetting } from '#lib/settings/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import AlreadyGoneToastContent from './AlreadyGoneToastContent.svelte'

/** Dedup id: two deletes finishing close together replace one note rather than stacking two. */
export const ALREADY_GONE_TOAST_ID = 'already-gone'

/** Five seconds, David's spec: long enough to read one sentence and reach the button. */
const ALREADY_GONE_TOAST_MS = 5000

/** The note's sentence: the item's name for one, a count for several. */
export function composeAlreadyGoneMessage(alreadyGone: AlreadyGone): string {
  if (alreadyGone.itemCount === 1 && alreadyGone.name !== null) {
    return tString('fileOperations.alreadyGone.one', { name: alreadyGone.name })
  }
  return tString('fileOperations.alreadyGone.many', {
    count: alreadyGone.itemCount,
    countText: formatNumber(alreadyGone.itemCount),
  })
}

/** Raises the note for a completion that found items already gone, unless the person turned it off. */
export function maybeShowAlreadyGoneToast(alreadyGone: AlreadyGone | null): void {
  if (alreadyGone === null || alreadyGone.itemCount <= 0) return
  if (!getSetting('fileOperations.showAlreadyGoneToast')) return
  addToast(AlreadyGoneToastContent, {
    id: ALREADY_GONE_TOAST_ID,
    level: 'info',
    timeoutMs: ALREADY_GONE_TOAST_MS,
    props: { message: composeAlreadyGoneMessage(alreadyGone) },
  })
}
