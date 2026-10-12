/**
 * The one toast every route into the background shows, in one shape: the
 * sentence saying where the job went, plus "Show in operation queue". The queue
 * window itself stays shut; the status corner's chip carries the job.
 *
 * Keyed to its job, so whatever later speaks for the job's ending (a trash's
 * Undo toast, the failure toast) replaces it via {@link dismissQueueHandoffToast}:
 * one keypress, one toast. A job that ends quietly (a copy) keeps it.
 */

import { addToast, dismissToast } from '#lib/ui/toast/index.js'
import { getMainWindowOperationRows } from './main-window-operations.svelte'
import QueueHandoffToastContent from './QueueHandoffToastContent.svelte'

export function queueHandoffToastId(operationId: string): string {
  return `queue-handoff:${operationId}`
}

export function showQueueHandoffToast(message: string, operationId: string): void {
  // A job that failed before this toast could show already has the failure
  // toast speaking for it; "Sent to the background" beside it is stale.
  const failed = getMainWindowOperationRows().some(
    (row) => row.snapshot.operationId === operationId && row.snapshot.status === 'failed',
  )
  if (failed) return
  addToast(QueueHandoffToastContent, {
    id: queueHandoffToastId(operationId),
    level: 'info',
    toastGroup: 'transfer-queue',
    props: { message },
  })
}

/** Takes `operationId`'s handoff toast down, if it's up. Called by the toast that replaces it. */
export function dismissQueueHandoffToast(operationId: string): void {
  dismissToast(queueHandoffToastId(operationId))
}
