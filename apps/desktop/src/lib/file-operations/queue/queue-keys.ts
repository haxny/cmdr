/**
 * What a keypress means in the operation queue window. Each key is a fixed-key registry
 * command in the `Operation queue window` scope (`lib/commands/sources/queue-window.ts`), so
 * the window asks the registry rather than testing key flags, and Settings and the Help window
 * list them. The page acts on the answer (`routes/queue/+page.svelte`).
 */

import type { CommandId } from '#lib/commands/index.js'
import { comboMatchesCommand, resolveKeyCombo } from '#lib/shortcuts/index.js'
import type { QueueMoveDirection } from '#lib/tauri-commands/index.js'
import { decisionKeyOf } from '../decision-keys'
import type { CursorStep } from './queue-cursor.svelte'

export type QueueKeyAction =
  | { kind: 'cursor'; to: CursorStep }
  | { kind: 'extend'; to: 'previous' | 'next' }
  | { kind: 'toggleSelected' }
  | { kind: 'selectAll' }
  | { kind: 'show' }
  | { kind: 'pauseOrResume' }
  | { kind: 'cancelOrDismiss' }
  | { kind: 'cancel' }
  | { kind: 'rollback' }
  | { kind: 'move'; direction: QueueMoveDirection }
  | { kind: 'control'; step: -1 | 1 }
  | { kind: 'close' }

/** The keys that act on the cursor row's own operation, through `QueueRowApi`. */
export type QueueRowAction = Extract<
  QueueKeyAction,
  { kind: 'show' | 'pauseOrResume' | 'cancelOrDismiss' | 'cancel' | 'rollback' | 'control' }
>

/** What a key does to ONE row's operation, exported by `QueueRow.svelte` next to the
 *  buttons that do the same. The page calls it on the cursor row. Each returns `false`
 *  where the row doesn't offer that button, so the key does nothing there. */
export interface QueueRowApi {
  focusRow: () => void
  /** Enter or S: Show. */
  show: () => boolean
  /** P: Pause, or Resume on a paused row. */
  pauseOrResume: () => boolean
  /** ⌫: Cancel, or Dismiss on a failure. */
  cancelOrDismiss: () => boolean
  /** C: Cancel. */
  cancel: () => boolean
  /** B: Rollback, which asks first, as the button does. */
  rollback: () => boolean
  /** ← / →: through the row's buttons and back to the row. */
  focusControl: (step: -1 | 1) => boolean
}

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
  ['queue.pauseOrResume', { kind: 'pauseOrResume' }],
  ['queue.cancelOrDismiss', { kind: 'cancelOrDismiss' }],
  ['queue.cancel', { kind: 'cancel' }],
  ['queue.rollback', { kind: 'rollback' }],
  ['queue.moveEarlier', { kind: 'move', direction: 'earlier' }],
  ['queue.moveLater', { kind: 'move', direction: 'later' }],
  ['queue.nextControl', { kind: 'control', step: 1 }],
  ['queue.previousControl', { kind: 'control', step: -1 }],
  ['queue.close', { kind: 'close' }],
]

/**
 * The combo a keypress stands for here, or `null`. A letter is read the way the decision
 * prompts read theirs (`decisionKeyOf`): bare, never a held key's repeat (a held P would
 * pause and resume in turn), and through the physical key on a non-Latin layout. A bare
 * letter that reading refuses means nothing here.
 */
function queueCombo(event: KeyboardEvent): string | null {
  const letter = decisionKeyOf(event)
  if (letter !== null && /^[A-Z]$/.test(letter)) return letter
  const combo = resolveKeyCombo(event)
  return /^[A-Z]$/.test(combo) ? null : combo
}

/** The window's reading of `event`, or `null` for a key it has no use for. */
export function queueKeyAction(event: KeyboardEvent): QueueKeyAction | null {
  // A key that finishes an IME composition belongs to whatever is composing.
  if (event.isComposing) return null
  const combo = queueCombo(event)
  if (combo === null) return null
  for (const [commandId, action] of KEY_ACTIONS) {
    if (comboMatchesCommand(combo, commandId)) return action
  }
  return null
}
