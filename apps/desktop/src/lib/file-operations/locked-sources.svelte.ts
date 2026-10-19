/**
 * The main window's host for "these items are Locked: move them anyway?".
 *
 * A move that finds Finder-Locked items (or ones whose permissions forbid
 * deleting them) among what it's about to move asks ONCE, before any byte moves
 * (#408): it emits `write-locked-sources` and parks until somebody answers Move
 * anyway or Skip, or cancels. Items the system locked are never asked about.
 * Backend half: `crates/cmdr-operations/src/locked_sources.rs`.
 *
 * What it does:
 *
 * - **Asks once nothing else is on screen**, one question at a time, like the
 *   sources-gone host (`sources-gone.svelte.ts`), because a backgrounded move's
 *   question would otherwise steal the keys of a dialog the person is typing in.
 *   While held, a quiet toast says a question is waiting.
 * - **Except the move's OWN progress dialog**: a foreground move asks right after
 *   its scan with its progress dialog up, and holding the question behind that
 *   dialog would hold it forever. That dialog is the one thing it stacks over.
 * - **The operation ends some other way** (Cancel in the queue window, the app
 *   tearing it down): the question goes, once the operation has been SEEN live.
 *
 * The queue, the wait, and the reconcile are `parked-question.svelte.ts`'s,
 * shared with the sources-gone host.
 */

import {
  answerLockedSources,
  cancelOperation,
  onWriteLockedSources,
  type LockedSourcesAnswer,
  type WriteLockedSourcesEvent,
} from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import { isAnySoftDialogOpenBesidesAll } from '#lib/ui/open-dialogs.svelte.js'
import { getForegroundOperationId } from './foreground-operation.svelte'
import type { OperationRow } from './queue/operations-store.svelte'
import { createParkedQuestionHost, type ParkedQuestionPrompt } from './parked-question.svelte'

const log = getAppLogger('locked-sources')

/** What the prompt renders. */
export type LockedSourcesPrompt = ParkedQuestionPrompt<WriteLockedSourcesEvent>

const host = createParkedQuestionHost<WriteLockedSourcesEvent>({
  log,
  label: 'Locked-items',
  // Busy while any OTHER dialog is on screen, the asking move's own progress
  // dialog aside. This prompt's own previous question is no reason to hold the
  // next: the host already keeps it to one at a time.
  isBusy: (asking) => {
    if (asking !== undefined && getForegroundOperationId() === asking.operationId) {
      return isAnySoftDialogOpenBesidesAll(['locked-sources', 'transfer-progress'])
    }
    return isAnySoftDialogOpenBesidesAll(['locked-sources'])
  },
  onHeld: () => {
    log.info('A move is waiting to ask about its Locked items; asking once the open dialog closes')
    addToast(tString('fileOperations.lockedSources.heldToast'), { level: 'info', timeoutMs: 10000 })
  },
})

/** The question on screen, or `null`. Reactive. */
export function getLockedSourcesPrompt(): LockedSourcesPrompt | null {
  return host.prompt()
}

/** An answer is in flight. Reactive. */
export function isAnsweringLockedSources(): boolean {
  return host.isAnswering()
}

function handleLockedSources(event: WriteLockedSourcesEvent): void {
  log.info('Op={operationId} found {count} Locked item(s); asking', {
    operationId: event.operationId,
    count: event.lockedCount,
  })
  host.ask(event)
}

function sendAnswer(choice: LockedSourcesAnswer): (operationId: string) => Promise<unknown> {
  return (operationId) => answerLockedSources(operationId, choice)
}

/** "Move anyway": the move unlocks each Locked item right before it takes it,
 *  and locks it again where it lands. Any outcome takes the question down:
 *  `not_asking` and `unknown_operation` mean the move went on without us. */
export async function moveLockedSourcesAnyway(): Promise<void> {
  await host.answer('move anyway', sendAnswer('move_anyway'))
}

/** "Skip": the Locked items stay where they are, and the rest moves. */
export async function skipLockedSources(): Promise<void> {
  await host.answer('skip', sendAnswer('skip'))
}

/** Ends the move through the ordinary cancel, which also drops its parked
 *  question on the backend. */
export async function cancelLockedSourcesPrompt(): Promise<void> {
  await host.answer('cancel', cancelOperation)
}

/**
 * Drops questions whose operation is no longer live. Exported so the pass is
 * callable straight from a test; the effect only decides when it runs.
 */
export function reconcileLockedSourcesPrompts(rows: OperationRow[]): void {
  host.reconcile(rows)
}

/** Starts listening. Idempotent; call {@link stopLockedSourcesHost} on teardown. */
export async function startLockedSourcesHost(): Promise<void> {
  await host.start(onWriteLockedSources, handleLockedSources)
}

/** Drops the listener, the effect, and every question. */
export function stopLockedSourcesHost(): void {
  host.stop()
}
