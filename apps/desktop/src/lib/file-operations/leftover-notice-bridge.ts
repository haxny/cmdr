/**
 * Kept-leftovers notice bridge.
 *
 * Turns the leftover sweep's `leftovers-kept` event into an info toast: a drive
 * came back carrying the working folder of a move that never finished, and Cmdr
 * left every file inside it exactly where it was; or a file on its way to its
 * name wasn't the size it should be, so Cmdr kept it under a ` (recovered)`
 * name rather than finish it.
 *
 * **Why this isn't part of a transfer's own toasts.** It belongs to no
 * operation. The move that made the folder ended in an earlier session, or
 * before the drive was unplugged, so there is no `operationId` to hang it off
 * and nothing on screen for it to join. The sweep speaks for itself
 * (`crates/cmdr-operations/src/in_flight_sweep.rs`).
 *
 * Keyed per folder (or kept file), so two found on one drive each get their own
 * notice and a re-plug can't stack a second copy of the same one. Mounted from
 * `routes/(main)/window-services.ts` beside the other event bridges.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { onLeftoversKept } from '#lib/tauri-commands/index.js'
import type { KeptLeftover, LeftoversKeptEvent } from '#lib/ipc/bindings.js'

const log = getAppLogger('fileOperations')

/** How long the notice stays up. Longer than a routine toast: it names
 * something the person may want to go and look at. */
const NOTICE_TIMEOUT_MS = 12_000

/** Mounts the listener. Returns its unsubscribe. */
export async function startLeftoverNoticeBridge(): Promise<UnlistenFn> {
  const unlisten = await onLeftoversKept(raiseNotice)
  log.debug('Kept-leftovers notice bridge mounted')
  return unlisten
}

function raiseNotice(payload: LeftoversKeptEvent): void {
  const { volumeName, kept } = payload
  log.info('An unfinished transfer left {kind} on {volumeName}, kept as {name}', {
    kind: kept.kind,
    volumeName,
    name: keptName(kept),
  })
  addToast(noticeText(volumeName, kept), {
    level: 'info',
    timeoutMs: NOTICE_TIMEOUT_MS,
    id: `leftovers-kept-${kept.kind}-${keptName(kept)}`,
  })
}

/** The name the kept thing has now: what the person would look for. */
function keptName(kept: KeptLeftover): string {
  switch (kept.kind) {
    case 'stagingFolder':
      return kept.folderName
    case 'unfinishedLanding':
      return kept.keptName
  }
}

function noticeText(volumeName: string, kept: KeptLeftover): string {
  switch (kept.kind) {
    case 'stagingFolder':
      return tString('fileOperations.leftovers.stagingFolderKept', { volumeName, folderName: kept.folderName })
    case 'unfinishedLanding':
      return tString('fileOperations.leftovers.unfinishedLandingKept', {
        volumeName,
        fileName: kept.fileName,
        keptName: kept.keptName,
      })
  }
}
