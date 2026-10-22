/**
 * Reaching the toasts from the keyboard, where `Tab` is otherwise the pane switch.
 *
 * "Move focus to the notifications" (`app.focusNotifications`, ⌃F6 like macOS's "Move focus
 * to the floating window") puts focus on the newest toast's first action: its first control
 * that isn't the close button, else an older toast's, else the newest toast's close button.
 * From there the arrows, Tab, and Shift+Tab cycle every toast's controls, newest first, and
 * wrap around, so focus stays on the toasts until Escape (back where focus came from) or the
 * last one closes. Home and End jump to the ends. Enter and Space stay with the focused button
 * (`toastStackOwnsKey` keeps them, and Tab, from the command dispatcher). Closing the focused
 * toast moves on to the toasts left, or back out once none is.
 *
 * Frames are no stops: a toast's text reaches a screen reader as the name of the group its
 * buttons sit in, and every toast has at least its close button.
 */

import { captureFocusOrigin, focusHome, restoreFocus, type FocusOrigin } from '../focus-restore'
import { claimKey } from '#lib/shortcuts/claim-key.js'
import { tString } from '#lib/intl/messages.svelte.js'

/** What the container hands over: its stack element and its polite region. */
export interface ToastStackHandle {
  element: () => HTMLElement | undefined
  announce: (message: string) => void
}

const FRAME_SELECTOR = '[data-toast-frame]'
/** What the walk visits inside a toast, in DOM order: the close button first, then the actions. */
const CONTROL_SELECTOR = 'button:not(:disabled), a[href]'
const CLOSE_SELECTOR = '.toast-close'
/** The keys focus in the stack keeps from the command dispatcher, as `formatKeyCombo` spells them. */
const OWN_KEYS = new Set(['Enter', 'Space', 'Tab', '⇧Tab', '↑', '↓', '←', '→', 'Home', 'End'])

let stack: ToastStackHandle | null = null
/** Where focus was when the command brought it here; null while the walk isn't on. */
let origin: FocusOrigin | null = null
let walking = false

/** Registers the window's toast stack; returns the unregister. One stack per window. */
export function registerToastStack(next: ToastStackHandle): () => void {
  stack = next
  return () => {
    if (stack === next) stack = null
  }
}

/** True when `target` sits in the toast stack. */
export function isInToastStack(target: EventTarget | null): boolean {
  const element = stack?.element()
  return !!element && target instanceof Node && element.contains(target)
}

/** Whether focus in the stack keeps `combo` from the command dispatcher (Enter on a button presses THAT button). */
export function toastStackOwnsKey(combo: string): boolean {
  return OWN_KEYS.has(combo) && isInToastStack(document.activeElement)
}

/** Every place the walk stops, top to bottom: each toast's controls, newest toast first. */
function stops(): HTMLElement[] {
  const element = stack?.element()
  if (!element) return []
  const frames = Array.from(element.querySelectorAll<HTMLElement>(FRAME_SELECTOR)).reverse()
  return frames.flatMap((frame) => Array.from(frame.querySelectorAll<HTMLElement>(CONTROL_SELECTOR)))
}

/** Where the walk starts: the first action that isn't a close button, else the newest close button. */
function entryStop(): HTMLElement | undefined {
  const all = stops()
  return all.find((stop) => !stop.matches(CLOSE_SELECTOR)) ?? all.at(0)
}

function focusStop(stop: HTMLElement): void {
  stop.focus({ preventScroll: true })
}

/**
 * Moves focus to the toasts (see the module comment for where it lands). Says "No
 * notifications" and leaves focus alone when no toast is up. False when there was nothing to
 * focus.
 */
export function focusNewestToast(): boolean {
  const entry = entryStop()
  if (!entry) {
    stack?.announce(tString('ui.toast.noneToFocus'))
    return false
  }
  if (!isInToastStack(document.activeElement)) origin = captureFocusOrigin()
  walking = true
  focusStop(entry)
  return true
}

/** Leaves the stack for where focus came from, or the window's home when that's gone. */
export function leaveToasts(): void {
  const from = origin
  origin = null
  walking = false
  const leaving = stack?.element()
  if (from?.kind === 'element' && !from.element.isConnected) {
    focusHome()
    return
  }
  if (from) restoreFocus(from, { leaving })
  else focusHome()
}

/**
 * The stack's keydown: the arrows, Tab, Home, and End walk the toasts, Escape leaves. Enter
 * and Space fall through to the focused button.
 */
export function handleToastStackKeydown(event: KeyboardEvent): void {
  if (event.metaKey || event.ctrlKey || event.altKey) return
  if (event.key === 'Escape') {
    claimKey(event)
    leaveToasts()
    return
  }
  const all = stops()
  const at = all.findIndex((stop) => stop === document.activeElement)
  const next = nextStop(event, at, all.length)
  if (next === null) return
  claimKey(event)
  const target = all.at(next)
  if (target) focusStop(target)
}

function nextStop(event: KeyboardEvent, at: number, count: number): number | null {
  if (count === 0) return null
  // Off every stop (a component toast's own focusable that isn't a button), the walk picks up at an end.
  const forward = at < 0 ? 0 : (at + 1) % count
  const back = at < 0 ? count - 1 : (at - 1 + count) % count
  switch (event.key) {
    case 'Tab':
      return event.shiftKey ? back : forward
    case 'ArrowDown':
    case 'ArrowRight':
      return forward
    case 'ArrowUp':
    case 'ArrowLeft':
      return back
    case 'Home':
      return 0
    case 'End':
      return count - 1
    default:
      return null
  }
}

/** The stack's focusout: focus leaving for somewhere else ends the walk. */
export function handleToastStackFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget
  if (next instanceof Node && !isInToastStack(next)) {
    origin = null
    walking = false
  }
}

/**
 * After a toast holding focus closed and focus fell to `<body>`: carry on the walk in the
 * toasts left, or leave the stack when it's empty. False when no walk was on, so the toast
 * puts focus back its own way (a pointer user's click).
 */
export function continueWalkAfterClose(): boolean {
  if (!walking) return false
  const entry = entryStop()
  if (entry) focusStop(entry)
  else leaveToasts()
  return true
}
