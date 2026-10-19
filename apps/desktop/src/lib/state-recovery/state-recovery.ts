/**
 * The notice that Cmdr couldn't read one of its own state files and started it fresh (#461, the
 * CRA's "report on corruptions"): which state reset, and where the old copy went.
 *
 * The backend sets the file aside and queues one `StateRecovery` per event
 * (`src-tauri/src/state_recovery/`). Most are found during launch, before this window exists, so
 * the queue holds them; this module drains it once at start and again on every
 * `state-recovery-queued` nudge. Draining is what makes it once: a recovery taken is gone, so a
 * later launch with a healthy file shows nothing.
 *
 * ⚠️ **Main window only.** The nudge reaches every window; `routes/(main)/window-services.ts` is
 * what scopes it.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { StateRecovery } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { onStateRecoveryQueued, takeStateRecoveries } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import StateRecoveryToastContent from './StateRecoveryToastContent.svelte'

const log = getAppLogger('stateRecovery')

let announced = 0

/** Shows one recovery. Each gets its own toast: two files breaking at once are two things to know. */
export function announceStateRecovery(recovery: StateRecovery): void {
  const id = `state-recovery:${recovery.file}:${String(announced++)}`
  addToast(StateRecoveryToastContent, {
    id,
    level: 'warn',
    dismissal: 'persistent',
    props: { recovery },
  })
}

async function drain(): Promise<void> {
  let recoveries: StateRecovery[]
  try {
    recoveries = await takeStateRecoveries()
  } catch (error) {
    // The queue keeps them, so the next nudge or launch tries again.
    log.warn("Couldn't fetch the recovered state files: {error}", { error })
    return
  }
  for (const recovery of recoveries) {
    announceStateRecovery(recovery)
  }
}

let unlisten: UnlistenFn | null = null

/** Start listening, then show what launch found. Call once at app init, from the main window only. */
export async function startStateRecoveryNotices(): Promise<void> {
  if (unlisten) return
  unlisten = await onStateRecoveryQueued(() => {
    void drain()
  })
  await drain()
}

export function stopStateRecoveryNotices(): void {
  if (!unlisten) return
  unlisten()
  unlisten = null
}
