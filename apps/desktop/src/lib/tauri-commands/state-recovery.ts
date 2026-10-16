// The notice for a state file Cmdr couldn't read and started fresh (#461): the backend queues one
// per recovery (`src-tauri/src/state_recovery/`), and the main window drains the queue.

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events, type StateRecovery } from '#lib/ipc/bindings.js'

/** Every recovery queued since the last call, oldest first. Draining, so each comes back once. */
export function takeStateRecoveries(): Promise<StateRecovery[]> {
  return commands.takeStateRecoveries()
}

/**
 * Subscribe to `state-recovery-queued`, the nudge that the queue holds something new. It carries
 * nothing: call `takeStateRecoveries` to get it.
 *
 * ⚠️ It reaches EVERY window. Subscribe from the main window only.
 */
export function onStateRecoveryQueued(callback: () => void): Promise<UnlistenFn> {
  return events.stateRecoveryQueued.listen(() => {
    callback()
  })
}
