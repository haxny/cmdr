/**
 * What the rail knows before anything is sent (the backend's `SlotStatus`): whether an AI
 * provider is set up, how the next call is priced, and the gauge's baseline. Read by the
 * provider notice, the composer's Send gate, and the footer.
 *
 * The backend answers from live state, so this only decides WHEN to ask: on the rail's mount,
 * after this window pushes AI config (`onAiConfigPushed`, so never before the change lands),
 * when the local server comes up or finishes installing, when the window regains focus (a
 * change made in the Settings window, which pushes from there), and when the rail says so
 * (a thread switch, a turn's end: memory and budget may have moved).
 */

import { onAiConfigPushed } from '#lib/settings/ai-config.js'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  askCmdrSlotStatus,
  onAiInstallComplete,
  onAiServerReady,
  type ProviderReadiness,
  type SlotStatus,
} from '#lib/tauri-commands/index.js'

const log = getAppLogger('askCmdr')

export const slotStatusState = $state<{ status: SlotStatus | null }>({ status: null })

// Each read is numbered so a slow one can't overwrite a newer answer.
let readSeq = 0

export async function refreshSlotStatus(): Promise<void> {
  const seq = ++readSeq
  try {
    const status = await askCmdrSlotStatus()
    // `null` means the backend's read broke: keep what was last known.
    if (status && seq === readSeq) slotStatusState.status = status
  } catch (e) {
    log.warn('reading the Ask Cmdr slot status failed: {error}', { error: e })
  }
}

/**
 * The setup gap a send would hit, or `null` when there is none. Unknown (not read yet) counts
 * as no gap: the backend refuses an unconfigured send on its own, and a disabled Send flashing
 * on every open would be worse than that rare refusal.
 */
export function providerGap(status: SlotStatus | null): Exclude<ProviderReadiness, 'ready'> | null {
  if (!status || status.readiness === 'ready') return null
  return status.readiness
}

/** Start refreshing on every trigger in the module docs. Returns the teardown. */
export function watchSlotStatus(): () => void {
  void refreshSlotStatus()
  const refresh = (): void => {
    void refreshSlotStatus()
  }
  const unsubscribes: (() => void)[] = [onAiConfigPushed(refresh)]
  window.addEventListener('focus', refresh)
  unsubscribes.push(() => {
    window.removeEventListener('focus', refresh)
  })
  let disposed = false
  for (const subscribe of [onAiServerReady, onAiInstallComplete]) {
    // Outside Tauri (a unit test with no event mock) there's no event bus; the other triggers
    // still refresh.
    try {
      void subscribe(refresh).then(
        (unlisten) => {
          if (disposed) unlisten()
          else unsubscribes.push(unlisten)
        },
        (e: unknown) => {
          log.debug('listening for AI server changes failed: {error}', { error: e })
        },
      )
    } catch (e) {
      log.debug('listening for AI server changes failed: {error}', { error: e })
    }
  }
  return () => {
    disposed = true
    for (const unsubscribe of unsubscribes) unsubscribe()
  }
}
