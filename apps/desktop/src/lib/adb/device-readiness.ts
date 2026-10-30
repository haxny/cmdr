/**
 * What a device's `DeviceReadiness` makes of its row in the volume switcher.
 *
 * ❗ **Readiness is PRESENCE, not session health.** A phone sitting on its
 * "Allow USB debugging?" prompt is answering the daemon; there is nothing to
 * reconnect and no backoff loop to start, which is why this is a separate field
 * from `connectionState` and a separate module from
 * `../file-explorer/navigation/connection-state.ts`.
 *
 * ❗ **A `waiting_for_authorization` row is OPENABLE.** Opening it is what puts
 * the pane on the waiting state that walks in by itself the moment the user
 * taps Allow. A disabled row there is the silence the ADB work exists to end,
 * and it is the one answer here that is easy to get backwards.
 *
 * ❗ **A `connecting` row is MTP's, and it is NOT openable.** Its session hasn't
 * opened, so there's no storage to land on and no dialer to walk a pane in; the
 * row turns into the storage's own the moment one is there. ❌ Never word it as
 * though a prompt is showing: the open can time out with none on screen.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import type { DeviceReadiness, DeviceUnavailableReason } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { VolumeInfo } from '#lib/file-explorer/types.js'

/** How the switcher draws one row, once readiness has had its say. */
export interface DeviceRowState {
  /** Whether activating the row navigates. `false` greys it out. */
  openable: boolean
  /** The finished sentence for the row's tooltip, or `null` for no tooltip. */
  tooltip: string | null
  /** What to do next, on a line of its own under the row, or `null` for no line. */
  status: string | null
}

/** A row nothing is holding back: every disk, and a phone that is ready. */
const UNREMARKABLE: DeviceRowState = { openable: true, tooltip: null, status: null }

/**
 * Why a greyed row is greyed, one sentence each.
 *
 * ❗ A `Record`, ❌ not a ternary: a third reason added to the backend enum would
 * otherwise inherit whichever branch the `else` happened to be, and a row would
 * quietly explain itself wrong. This way it doesn't compile.
 */
const UNAVAILABLE_KEYS: Record<DeviceUnavailableReason, MessageKey> = {
  offline: 'adb.readiness.offline',
  no_permissions: 'adb.readiness.noPermissions',
}

/** How to draw the row for `readiness`. `null` is a volume that is not a device. */
export function deviceRowState(readiness: DeviceReadiness | null | undefined): DeviceRowState {
  if (!readiness) return UNREMARKABLE
  switch (readiness.kind) {
    case 'ready':
      return UNREMARKABLE
    case 'waiting_for_authorization':
      return {
        openable: true,
        tooltip: tString('adb.readiness.waitingForAuthorization'),
        status: tString('adb.readiness.tapAllow'),
      }
    case 'connecting':
      return {
        openable: false,
        tooltip: tString('mtp.readiness.connectingTooltip'),
        status: tString('mtp.readiness.connecting'),
      }
    case 'unavailable':
      return { openable: false, tooltip: tString(UNAVAILABLE_KEYS[readiness.reason]), status: null }
  }
}

/**
 * Whether this row stands for a phone whose MTP session hasn't opened: on the list so the user
 * sees it, but not a place yet. ❗ Anything that picks a volume to go to or to write to asks this
 * first (the palette, MCP, the transfer dialog's destinations); so do eject and the index badge.
 */
export function isStillConnecting(volume: Pick<VolumeInfo, 'deviceReadiness'>): boolean {
  return volume.deviceReadiness?.kind === 'connecting'
}
