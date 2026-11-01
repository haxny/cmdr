/**
 * An open dialog's claim on a native menu command whose accelerator is one of the
 * dialog's own keys: the Multi-rename sheet's F2 (open its Presets menu) is also
 * File > Rename's accelerator.
 *
 * Whether AppKit hands such a key to the webview first or runs the menu item first isn't
 * settled for every key (`routes/(main)/DETAILS.md` § Native-menu and input-focus
 * interactions), and a disabled item's accelerator still fires. So the dialog answers
 * its key on both roads: its own keydown, and this claim, which the dispatch core runs
 * for the command's MENU road ahead of the dialog gate (`command-dispatch.ts`). With
 * one destination, the order stops mattering; the dialog drops the second half of a
 * double fire itself.
 *
 * One claim per command, the newest winning; a dialog claims on mount and releases on
 * destroy.
 */

import type { CommandId } from './command-ids'

/** A claim's handler. Answering `false` declines this fire, so the command runs as if unclaimed (⌘C in a text field). */
type MenuClaim = () => unknown

const claims = new Map<CommandId, MenuClaim>()

/** Sends `commandId`'s menu road to `run` until the returned release is called. */
export function claimMenuCommand(commandId: CommandId, run: MenuClaim): () => void {
  claims.set(commandId, run)
  return () => {
    // A newer claim (a remount before the old destroy ran) stays.
    if (claims.get(commandId) === run) claims.delete(commandId)
  }
}

/** Runs `commandId`'s claim, if a dialog holds one. `true` when it took the fire (it didn't decline). */
export function runMenuClaim(commandId: CommandId): boolean {
  const run = claims.get(commandId)
  if (!run) return false
  return run() !== false
}

/** The two roads one keypress can arrive by: a dialog's keydown, and a menu item's accelerator. */
export type KeyRoad = 'keyboard' | 'menu'

/**
 * The two fires of one keypress land milliseconds apart; a person can't press the key and
 * pick the menu item inside this. Same window as the dispatch core's (`dispatch-dedup.ts`).
 */
const ECHO_WINDOW_MS = 300

/**
 * Tells the second half of a keyboard + menu double fire from a real press: the OTHER
 * road within the window is the echo, while a second press on the same road (a quick
 * double tap, key repeat) always counts. An echo doesn't extend the window. A key that
 * toggles something would undo itself if an echo got through.
 */
export function createKeyRoadEcho(now: () => number = Date.now): (road: KeyRoad) => boolean {
  let last: { road: KeyRoad; at: number } | null = null
  return (road) => {
    const at = now()
    if (last !== null && last.road !== road && at - last.at < ECHO_WINDOW_MS) return true
    last = { road, at }
    return false
  }
}
