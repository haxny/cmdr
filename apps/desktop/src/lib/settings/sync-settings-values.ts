/**
 * The stored values of the Synchronize folders settings (a part of `SettingsValues`, kept here so `types.ts` stays
 * short). Read by the dialog at compare and at run and passed to the backend as `SyncDefaults`; a profile may
 * override each one. The versions location is two settings plus a name or template; `keepPolicyFromSettings`
 * builds the backend's `KeepPolicy` from them.
 */
export interface SyncSettingsValues {
  'sync.keep.policy': 'trashElseVersions' | 'versionsAlways' | 'trashOnly'
  'sync.keep.location': 'targetRoot' | 'customFolder'
  'sync.keep.folderName': string
  'sync.keep.customFolder': string
  'sync.compare.timeTolerance': number
  'sync.compare.ignoreHourShift': boolean
  'sync.guards.maxRemovePercent': number
  'sync.guards.maxRemoveCount': number
}
