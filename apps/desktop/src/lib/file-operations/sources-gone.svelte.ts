/**
 * The main window's host for "sources left while this operation queued".
 *
 * An operation that waited behind another one checks its top-level sources when
 * its turn comes. When some are gone (the person moved or deleted them while it
 * waited), the backend emits `write-sources-gone` and parks the operation until
 * somebody says "do the rest" or cancels; when ALL are gone it has already
 * completed and the event only says why (ERR-962JD, where the queued move just
 * stopped with "no longer exists"). Backend half:
 * `crates/cmdr-operations/src/queued_sources.rs`.
 *
 * What it does:
 *
 * - **Some gone**: queues the question and asks once nothing else is on screen,
 *   one question at a time. A queued job is a background job by nature, so it
 *   follows the late-dialog rule (`#lib/file-explorer/pane/when-dialogs-clear`):
 *   a prompt stacked over the dialog the person is typing in would steal their
 *   keys. While held, a quiet toast says a question is waiting.
 * - **All gone**: an info toast saying so plainly. ❌ No error tone: nothing went
 *   wrong.
 * - **The operation ends some other way** (Cancel in the queue window, the app
 *   tearing it down): the question goes, once the operation has been SEEN live.
 *
 * The queue, the wait, and the reconcile are `parked-question.svelte.ts`'s,
 * shared with the Locked-items host.
 *
 * Unlike the conflict host it pauses nothing: the asking operation is the only
 * one parked, and the rest of the queue has no reason to wait for this answer.
 */

import {
  cancelOperation,
  continueWithoutGoneSources,
  onWriteSourcesGone,
  type WriteSourcesGoneEvent,
} from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import { isAnySoftDialogOpenBesides } from '#lib/ui/open-dialogs.svelte.js'
import type { OperationRow } from './queue/operations-store.svelte'
import { createParkedQuestionHost, type ParkedQuestionPrompt } from './parked-question.svelte'

const log = getAppLogger('sources-gone')

/** What the prompt renders. */
export type SourcesGonePrompt = ParkedQuestionPrompt<WriteSourcesGoneEvent>

const host = createParkedQuestionHost<WriteSourcesGoneEvent>({
  log,
  label: 'sources-gone',
  // Busy while any OTHER dialog is on screen. This prompt's own previous
  // question is no reason to hold the next: the host already keeps it to one at
  // a time, and the answer takes the old one down before the next goes up.
  isBusy: () => isAnySoftDialogOpenBesides('sources-gone'),
  onHeld: (event) => {
    log.info('A queued {type} is waiting to ask about its sources; asking once the open dialog closes', {
      type: event.operationType,
    })
    addToast(tString('fileOperations.sourcesGone.heldToast', { type: event.operationType }), {
      level: 'info',
      timeoutMs: 10000,
    })
  },
})

/** The question on screen, or `null`. Reactive. */
export function getSourcesGonePrompt(): SourcesGonePrompt | null {
  return host.prompt()
}

/** An answer is in flight. Reactive. */
export function isAnsweringSourcesGone(): boolean {
  return host.isAnswering()
}

function handleSourcesGone(event: WriteSourcesGoneEvent): void {
  if (event.next === 'nothing_left') {
    log.info('Queued op={operationId} found all {count} source(s) gone; nothing to do', {
      operationId: event.operationId,
      count: event.totalCount,
    })
    addToast(
      tString('fileOperations.sourcesGone.nothingLeftToast', {
        type: event.operationType,
        total: event.totalCount,
        totalText: formatInteger(event.totalCount),
      }),
      { level: 'info' },
    )
    return
  }
  log.info('Queued op={operationId} found {gone} of {total} source(s) gone; asking', {
    operationId: event.operationId,
    gone: event.goneCount,
    total: event.totalCount,
  })
  host.ask(event)
}

/** "Move the rest" (or copy, delete, …): carries the operation on with the
 *  sources still there. Any outcome takes the question down: `not_asking` and
 *  `unknown_operation` mean the operation moved on without us. */
export async function continueSourcesGonePrompt(): Promise<void> {
  await host.answer('continue', continueWithoutGoneSources)
}

/** Ends the operation through the ordinary cancel, which also drops its parked
 *  question on the backend. */
export async function cancelSourcesGonePrompt(): Promise<void> {
  await host.answer('cancel', cancelOperation)
}

/**
 * Drops questions whose operation is no longer live. Exported so the pass is
 * callable straight from a test; the effect only decides when it runs.
 */
export function reconcileSourcesGonePrompts(rows: OperationRow[]): void {
  host.reconcile(rows)
}

/** Starts listening. Idempotent; call {@link stopSourcesGoneHost} on teardown. */
export async function startSourcesGoneHost(): Promise<void> {
  await host.start(onWriteSourcesGone, handleSourcesGone)
}

/** Drops the listener, the effect, and every question. */
export function stopSourcesGoneHost(): void {
  host.stop()
}
