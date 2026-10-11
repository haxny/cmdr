/**
 * The one toast every route into the background shows, in one shape: the
 * sentence saying where the job went, plus "Show in operation queue". The queue
 * window itself stays shut; the status corner's chip carries the job.
 */

import { addToast } from '$lib/ui/toast'
import QueueHandoffToastContent from './QueueHandoffToastContent.svelte'

export function showQueueHandoffToast(message: string): void {
  addToast(QueueHandoffToastContent, { level: 'info', toastGroup: 'transfer-queue', props: { message } })
}
