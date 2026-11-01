/**
 * The app's side of the dialog's seams: the IPC wrappers the session calls, and the Synchronize folders Settings
 * read as the `SyncDefaults` every compare passes (Settings are frontend-read; the backend validates them).
 */
import {
  cancelSyncCompare,
  cancelSyncRun,
  deleteSyncProfile,
  getSyncConfirm,
  getSyncProfiles,
  moveSyncProfile,
  onSyncRunFinished,
  onSyncRunProgress,
  renameSyncProfile,
  saveSyncProfile,
  setSyncRunPaused,
  startSyncRun,
  closeSyncSession,
  compareSyncItemContent,
  findSyncRow,
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
} from '#lib/tauri-commands/sync.js'
import { getSetting } from '#lib/settings/settings-store.js'
import { keepPolicyFromSettings } from '#lib/settings/sync-keep-policy.js'
import type { SyncIpc } from './sync-session.svelte'
import type { SyncRunIpc } from './sync-run.svelte'
import type { SyncProfilesIpc } from './profiles.svelte'

/** Everything the dialog asks of the backend: the compare session, the run, and the profiles. */
export type SyncDialogIpc = SyncIpc & SyncRunIpc & SyncProfilesIpc

export const syncIpc: SyncDialogIpc = {
  startSyncCompare,
  setSyncComparePaused,
  cancelSyncCompare,
  getSyncRows,
  getSyncState,
  setSyncActions,
  findSyncRow,
  compareSyncItemContent,
  closeSyncSession,
  getSyncLastOptions,
  saveSyncLastOptions,
  onSyncCompareProgress,
  onSyncCompareFinished,
  onSyncRowsChanged,
  getSyncConfirm,
  startSyncRun,
  setSyncRunPaused,
  cancelSyncRun,
  onSyncRunProgress,
  onSyncRunFinished,
  getSyncProfiles,
  saveSyncProfile,
  deleteSyncProfile,
  renameSyncProfile,
  moveSyncProfile,
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
