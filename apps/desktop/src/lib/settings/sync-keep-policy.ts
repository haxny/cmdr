/**
 * The backend's `KeepPolicy` from the four Synchronize folders settings that name it. The dialog passes the
 * result to a compare and a run; the section passes it to `checkSyncKeepPolicy` to show a problem next to the
 * field. Pure: the settings arrive as arguments.
 */
import type { KeepPolicy } from '#lib/ipc/bindings.js'

export interface KeepSettings {
  policy: 'trashElseVersions' | 'versionsAlways' | 'trashOnly'
  location: 'targetRoot' | 'customFolder'
  folderName: string
  customFolder: string
}

export function keepPolicyFromSettings(settings: KeepSettings): KeepPolicy {
  return {
    place: settings.policy,
    location:
      settings.location === 'customFolder'
        ? { custom: { template: settings.customFolder } }
        : { targetRoot: { folderName: settings.folderName } },
  }
}
