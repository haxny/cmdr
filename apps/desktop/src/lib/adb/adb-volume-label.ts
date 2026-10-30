/**
 * The volume switcher's label for a device volume. The same phone can be listed
 * twice, over MTP and over ADB, under the same model name; the ADB entry gets an
 * "ADB" suffix only then, so a phone that is only reachable one way keeps its
 * plain name. Contract: `docs/specs/android-adb-backend.md` § "App wiring".
 */
import type { VolumeInfo } from '#lib/file-explorer/types.js'
import { tString } from '#lib/intl/messages.svelte.js'

/** Whether this is the ADB half of a device also listed over MTP. */
function hasMtpTwin(volume: VolumeInfo, volumes: readonly VolumeInfo[]): boolean {
  return volumes.some(
    (v) => v.id !== volume.id && v.category === 'mobile_device' && v.fsType !== 'adb' && v.name === volume.name,
  )
}

/**
 * Whether the switcher leaves this row out because the phone already has a row it can open:
 * the ADB half of a phone listed over MTP, while that half can't be used (it owes the Allow
 * tap, or the daemon can't reach it). It comes back the moment it turns ready.
 *
 * ❗ The switcher's call alone: the row stays in the volume list, so a pane waiting on it still
 * walks in by itself when the user taps Allow. Once both halves can be opened, both show (one
 * row per protocol, decided in #192).
 */
export function deviceRowDefersToMtpTwin(volume: VolumeInfo, volumes: readonly VolumeInfo[]): boolean {
  if (volume.fsType !== 'adb' || !volume.deviceReadiness || volume.deviceReadiness.kind === 'ready') return false
  return hasMtpTwin(volume, volumes)
}

/** The switcher label: the volume's name, suffixed with "ADB" when an MTP twin shares it. */
export function deviceVolumeLabel(volume: VolumeInfo, volumes: readonly VolumeInfo[]): string {
  if (volume.fsType !== 'adb' || !hasMtpTwin(volume, volumes)) return volume.name
  return tString('adb.volumeLabelWithSuffix', { deviceName: volume.name })
}
