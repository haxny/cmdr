/**
 * Reaching the toasts from the keyboard, without giving up `Tab` (the pane switch).
 *
 * "Move focus to the notifications" (`app.focusNotifications`, ⌃F6 like macOS's "Move focus
 * to the floating window") puts focus on the newest toast's frame, a `role="group"` named by
 * what the toast says. From there the arrows walk every toast, newest first, and each one's
 * buttons; Home and End jump to the ends. Enter and Space stay with the focused button
 * (`toastStackOwnsKey` keeps them from the command dispatcher), and Escape goes back where
 * focus came from. Closing the focused toast moves on to the newest one left, or back out
 * once none is.
 *
 * A frame is focusable only while the walk is on it: its `tabindex` comes and goes with
 * focus, because WebKit makes any element with a `tabindex` mouse-focusable, and a click on a
 * toast's text must not pull focus off the file list.
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
/** What the walk visits inside a toast, after its frame. */
const CONTROL_SELECTOR = 'button:not(:disabled), a[href]'
/** The keys focus in the stack keeps from the command dispatcher, as `formatKeyCombo` spells them. */
const OWN_KEYS = new Set(['Enter', 'Space', '↑', '↓', '←', '→', 'Home', 'End'])

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

/** Every place the walk stops, top to bottom: each toast's frame, then its controls. Newest first. */
function stops(): HTMLElement[] {
  const element = stack?.element()
  if (!element) return []
  const frames = Array.from(element.querySelectorAll<HTMLElement>(FRAME_SELECTOR)).reverse()
  return frames.flatMap((frame) => [frame, ...Array.from(frame.querySelectorAll<HTMLElement>(CONTROL_SELECTOR))])
}

function focusStop(stop: HTMLElement): void {
  if (stop.matches(FRAME_SELECTOR)) {
    stop.setAttribute('tabindex', '-1')
    stop.addEventListener(
      'blur',
      () => {
        stop.removeAttribute('tabindex')
      },
      { once: true },
    )
  }
  stop.focus({ preventScroll: true })
}

/**
 * Moves focus to the newest toast. Says "No notifications" and leaves focus alone when no
 * toast is up. False when there was nothing to focus.
 */
export function focusNewestToast(): boolean {
  const newest = stops().at(0)
  if (!newest) {
    stack?.announce(tString('ui.toast.noneToFocus'))
    return false
  }
  if (!isInToastStack(document.activeElement)) origin = captureFocusOrigin()
  walking = true
  focusStop(newest)
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
 * The stack's keydown: the arrows, Home, and End walk the toasts, Escape leaves. Enter and
 * Space fall through to the focused button.
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
  const next = nextStop(event.key, at, all.length)
  if (next === null) return
  claimKey(event)
  const target = all.at(next)
  if (target) focusStop(target)
}

function nextStop(key: string, at: number, count: number): number | null {
  switch (key) {
    case 'ArrowDown':
    case 'ArrowRight':
      return Math.min(at + 1, count - 1)
    case 'ArrowUp':
    case 'ArrowLeft':
      return Math.max(at - 1, 0)
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
 * After a toast holding focus closed and focus fell to `<body>`: carry on the walk at the
 * newest toast left, or leave the stack when it's empty. False when no walk was on, so the
 * toast puts focus back its own way (a pointer user's click).
 */
export function continueWalkAfterClose(): boolean {
  if (!walking) return false
  const newest = stops().at(0)
  if (newest) focusStop(newest)
  else leaveToasts()
  return true
}
