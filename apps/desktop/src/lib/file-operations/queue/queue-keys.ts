/**
 * What a keypress means in the operation queue window. Each key is a fixed-key registry
 * command in the `Operation queue window` scope (`lib/commands/sources/queue-window.ts`), so
 * the window asks the registry rather than testing key flags, and Settings and the Help window
 * list them. The page acts on the answer (`routes/queue/+page.svelte`).
 */

import type { CommandId } from '$lib/commands'
import { comboMatchesCommand, resolveKeyCombo } from '$lib/shortcuts'
import type { QueueMoveDirection } from '$lib/tauri-commands'
import type { CursorStep } from './queue-cursor.svelte'

export type QueueKeyAction =
  | { kind: 'cursor'; to: CursorStep }
  | { kind: 'extend'; to: 'previous' | 'next' }
  | { kind: 'toggleSelected' }
  | { kind: 'selectAll' }
  | { kind: 'show' }
  | { kind: 'cancelOrDismiss' }
  | { kind: 'move'; direction: QueueMoveDirection }
  | { kind: 'control'; step: -1 | 1 }
  | { kind: 'close' }

/** Each command and what it asks the window to do. Every combo is matched whole, so ⇧↑ is
 *  never ↑ and ⌥⌘A is never ⌘A. */
const KEY_ACTIONS: readonly (readonly [CommandId, QueueKeyAction])[] = [
  ['queue.cursorUp', { kind: 'cursor', to: 'previous' }],
  ['queue.cursorDown', { kind: 'cursor', to: 'next' }],
  ['queue.cursorFirst', { kind: 'cursor', to: 'first' }],
  ['queue.cursorLast', { kind: 'cursor', to: 'last' }],
  ['queue.extendUp', { kind: 'extend', to: 'previous' }],
  ['queue.extendDown', { kind: 'extend', to: 'next' }],
  ['queue.toggleSelected', { kind: 'toggleSelected' }],
  ['queue.selectAll', { kind: 'selectAll' }],
  ['queue.showInMainWindow', { kind: 'show' }],
  ['queue.cancelOrDismiss', { kind: 'cancelOrDismiss' }],
  ['queue.moveEarlier', { kind: 'move', direction: 'earlier' }],
  ['queue.moveLater', { kind: 'move', direction: 'later' }],
  ['queue.nextControl', { kind: 'control', step: 1 }],
  ['queue.previousControl', { kind: 'control', step: -1 }],
  ['queue.close', { kind: 'close' }],
]

/** The window's reading of `event`, or `null` for a key it has no use for. */
export function queueKeyAction(event: KeyboardEvent): QueueKeyAction | null {
  // A key that finishes an IME composition belongs to whatever is composing.
  if (event.isComposing) return null
  const combo = resolveKeyCombo(event)
  for (const [commandId, action] of KEY_ACTIONS) {
    if (comboMatchesCommand(combo, commandId)) return action
  }
  return null
}
