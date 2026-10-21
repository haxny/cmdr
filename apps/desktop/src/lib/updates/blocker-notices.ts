/**
 * What the updater says to an install that can't write its own bundle, and when.
 *
 * Two notices, picked by the blocker:
 *
 * - **Where the bundle sits** (translocated, or on a read-only volume): the "move Cmdr to Applications" dialog
 *   (`updateBlockerNotice`, mounted by the main layout). At most once per session.
 * - **Who may write it** (a bundle this user can't write, in practice a root-owned `.pkg` install): a persistent warn
 *   toast saying what the person CAN do, which depends on whether they're an administrator. Once per offered version.
 *
 * Both are held while onboarding owns the screen and raised when it closes (`flushPendingBlockerNotices`). The updater
 * owns that gate and passes it in, so this module keeps no onboarding state of its own.
 */

import { updateBundlePath, updateWriteBlocker } from '#lib/tauri-commands/index.js'
import type { BundleWriteBlocker } from '#lib/tauri-commands/index.js'
import { getSetting, setSetting } from '#lib/settings/settings-store.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import UpdateUnwritableToastContent from './UpdateUnwritableToastContent.svelte'
import {
  isPermissionBlocker,
  updateBlockerNotice,
  type LocationBlocker,
  type UnwritableUpdate,
} from './update-state.svelte'

const log = getAppLogger('updater')

/**
 * Asks the backend whether this install can write its own bundle.
 *
 * A failure to ANSWER is not a blocker. The classification is a courtesy that saves a doomed
 * download; treating an IPC hiccup as "can't update" would stop updates that would have worked.
 * macOS-only, like the rest of `runMacUpdateFlow`: the Tauri plugin owns the install elsewhere.
 */
export async function readWriteBlocker(): Promise<BundleWriteBlocker | null> {
  try {
    return await updateWriteBlocker()
  } catch (error) {
    log.warn("Couldn't classify where the bundle lives: {error}", {
      error,
    })
    return null
  }
}

/**
 * Raises the notice for `blocker`, or holds it until `flushPendingBlockerNotices` when `screenFree` is false
 * (onboarding owns the screen).
 */
export function raiseBlockerNotice(
  { blocker, version }: { blocker: BundleWriteBlocker; version: string },
  screenFree: boolean,
): void {
  if (isPermissionBlocker(blocker)) {
    showUnwritableNotice({ version, blocker }, screenFree)
  } else {
    showMoveToApplicationsNudge(blocker, screenFree)
  }
}

/** Raises whatever onboarding held back. The updater calls it once the screen is free. */
export function flushPendingBlockerNotices(): void {
  if (pendingMoveNudge !== null) showMoveToApplicationsNudge(pendingMoveNudge, true)
  if (pendingUnwritableNotice !== null) showUnwritableNotice(pendingUnwritableNotice, true)
}

/**
 * The "move Cmdr to Applications" nudge is raised at most once per session: a modal reappearing
 * every poll interval would be its own problem, and the answer doesn't change until the user does
 * something about it. `pendingMoveNudge` holds one that arrived while onboarding owned the screen,
 * which is exactly the population this nudge is for (a download opened straight from `~/Downloads`
 * is what makes macOS translocate it), so it's remembered rather than dropped.
 */
let moveNudgeShown = false
let pendingMoveNudge: LocationBlocker | null = null

function showMoveToApplicationsNudge(blocker: LocationBlocker, screenFree: boolean): void {
  if (moveNudgeShown) return
  if (!screenFree) {
    pendingMoveNudge = blocker
    return
  }
  moveNudgeShown = true
  pendingMoveNudge = null
  updateBlockerNotice.blocker = blocker
}

/** Closes the nudge. It doesn't come back this session; the next launch asks again. */
export function dismissMoveToApplicationsNudge(): void {
  updateBlockerNotice.blocker = null
}

/**
 * The unwritable-bundle notice says its piece once per offered VERSION, across relaunches: the person can't fix it from
 * inside Cmdr (only by replacing the app, or by asking whoever manages the Mac), so repeating it every launch would be
 * nagging, while a newer release is news worth one more toast. `updates.unwritableNoticeVersion` is the stamp, written
 * when the toast actually shows. One that arrives during onboarding waits in `pendingUnwritableNotice`.
 */
let pendingUnwritableNotice: UnwritableUpdate | null = null

function showUnwritableNotice(notice: UnwritableUpdate, screenFree: boolean): void {
  if (getSetting('updates.unwritableNoticeVersion') === notice.version) return
  if (!screenFree) {
    pendingUnwritableNotice = notice
    return
  }
  pendingUnwritableNotice = null
  setSetting('updates.unwritableNoticeVersion', notice.version)
  void raiseUnwritableToast(notice)
}

async function raiseUnwritableToast({ version, blocker }: UnwritableUpdate): Promise<void> {
  let bundlePath: string | null = null
  try {
    bundlePath = await updateBundlePath()
  } catch (error) {
    log.warn("Couldn't read the bundle path for the unwritable-bundle notice: {error}", { error })
  }
  addToast(UpdateUnwritableToastContent, {
    id: 'update-unwritable',
    level: 'warn',
    dismissal: 'persistent',
    // Outside a bundle the notice can't show at all (no check runs there), so the fallback is the usual install path.
    props: { version, blocker, bundlePath: bundlePath ?? '/Applications/Cmdr.app' },
  })
}

/** Test-only hook: forget what this session already said and held. */
export function _resetBlockerNoticesForTest(): void {
  moveNudgeShown = false
  pendingMoveNudge = null
  pendingUnwritableNotice = null
  updateBlockerNotice.blocker = null
}
