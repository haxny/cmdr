/**
 * The app's side of the dialog's seams: the IPC wrappers the session calls, and the Synchronize folders Settings
 * read as the `SyncDefaults` every compare passes (Settings are frontend-read; the backend validates them).
 */
import {
  cancelSyncCompare,
  closeSyncSession,
  compareSyncItemContent,
  getSyncLastOptions,
  getSyncRows,
  getSyncState,
  onSyncCompareFinished,
  onSyncCompareProgress,
  onSyncRowsChanged,
  saveSyncLastOptions,
  setSyncActions,
  setSyncComparePaused,
  startSyncCompare,
  type SyncDefaults,
} from '#lib/tauri-commands/index.js'
import { getSetting } from '#lib/settings/settings-store.js'
import { keepPolicyFromSettings } from '#lib/settings/sync-keep-policy.js'
import type { SyncIpc } from './sync-session.svelte'

export const syncIpc: SyncIpc = {
  startSyncCompare,
  setSyncComparePaused,
  cancelSyncCompare,
  getSyncRows,
  getSyncState,
  setSyncActions,
  compareSyncItemContent,
  closeSyncSession,
  getSyncLastOptions,
  saveSyncLastOptions,
  onSyncCompareProgress,
  onSyncCompareFinished,
  onSyncRowsChanged,
}

/** The Settings values a compare starts from. The sentinel file is a profile override only. */
export function readSyncDefaults(): SyncDefaults {
  return {
    keep: keepPolicyFromSettings({
      policy: getSetting('sync.keep.policy'),
      location: getSetting('sync.keep.location'),
      folderName: getSetting('sync.keep.folderName'),
      customFolder: getSetting('sync.keep.customFolder'),
    }),
    timeToleranceSecs: getSetting('sync.compare.timeTolerance'),
    ignoreHourShift: getSetting('sync.compare.ignoreHourShift'),
    guards: {
      maxRemovePercent: getSetting('sync.guards.maxRemovePercent'),
      maxRemoveCount: getSetting('sync.guards.maxRemoveCount'),
      sentinel: null,
    },
  }
}
