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
 * Unlike the conflict host it pauses nothing: the asking operation is the only
 * one parked, and the rest of the queue has no reason to wait for this answer.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  cancelOperation,
  continueWithoutGoneSources,
  onWriteSourcesGone,
  type OperationSnapshot,
  type WriteSourcesGoneEvent,
} from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import { isAnySoftDialogOpenBesides } from '#lib/ui/open-dialogs.svelte.js'
import { createWhenDialogsClear } from '#lib/file-explorer/pane/when-dialogs-clear.svelte.js'
import { getMainWindowOperationRows } from './queue/main-window-operations.svelte'
import { isTerminalStatus, type OperationRow } from './queue/operations-store.svelte'

const log = getAppLogger('sources-gone')

/** A question waiting for an answer. */
interface Entry {
  event: WriteSourcesGoneEvent
  /** True once the operation has been seen live in a snapshot. Until then its
   *  absence from the rows means "hasn't arrived yet", not "gone". */
  confirmedLive: boolean
}

/** What the prompt renders. */
export interface SourcesGonePrompt {
  operationId: string
  event: WriteSourcesGoneEvent
  /** The operation's row, for naming which transfer is asking. `null` only in
   *  the sliver before its snapshot lands. */
  snapshot: OperationSnapshot | null
}

/** Unanswered questions, oldest first. */
let entries = $state<Entry[]>([])
/** The operation whose question is on screen, or `null`. */
let shownId = $state<string | null>(null)
/** An answer is in flight; the buttons disable. */
let answering = $state(false)
/** A question is already waiting for the window to free up. */
let offerPending = false

let unlisten: UnlistenFn | null = null
let stopEffects: (() => void) | null = null

/** Busy while any OTHER dialog is on screen. This prompt's own previous
 *  question is no reason to hold the next: `shownId` already keeps it to one at
 *  a time, and the answer takes the old one down before the next goes up. */
const whenDialogsClear = createWhenDialogsClear(() => isAnySoftDialogOpenBesides('sources-gone'))

/** The question on screen, or `null`. Reactive. */
export function getSourcesGonePrompt(): SourcesGonePrompt | null {
  if (shownId === null) return null
  const entry = entries.find((e) => e.event.operationId === shownId)
  if (!entry) return null
  const snapshot = getMainWindowOperationRows().find((r) => r.snapshot.operationId === shownId)?.snapshot ?? null
  return { operationId: shownId, event: entry.event, snapshot }
}

/** An answer is in flight. Reactive. */
export function isAnsweringSourcesGone(): boolean {
  return answering
}

/** Shows the oldest waiting question once the window is free. */
function offerNext(): void {
  if (shownId !== null || offerPending || entries.length === 0) return
  offerPending = true
  const askedNow = whenDialogsClear.run(() => {
    offerPending = false
    shownId = entries[0]?.event.operationId ?? null
  })
  if (!askedNow) {
    const operationType = entries[0].event.operationType
    log.info('A queued {type} is waiting to ask about its sources; asking once the open dialog closes', {
      type: operationType,
    })
    addToast(tString('fileOperations.sourcesGone.heldToast', { type: operationType }), {
      level: 'info',
      timeoutMs: 10000,
    })
  }
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
  // One question per operation: a repeat replaces it rather than asking twice.
  const others = entries.filter((e) => e.event.operationId !== event.operationId)
  entries = [...others, { event, confirmedLive: false }]
  offerNext()
}

/** Takes `operationId`'s question down and moves on to the next one. */
function drop(operationId: string): void {
  entries = entries.filter((e) => e.event.operationId !== operationId)
  if (shownId === operationId) shownId = null
  offerNext()
}

/** Runs one answer. A call that never landed leaves the question up: the
 *  operation is still waiting for it. */
async function answer(how: string, send: (operationId: string) => Promise<unknown>): Promise<void> {
  const operationId = shownId
  if (operationId === null || answering) return
  answering = true
  try {
    const outcome = await send(operationId)
    log.info('Sources-gone question for op={operationId} answered with {how}: {outcome}', {
      operationId,
      how,
      outcome: String(outcome),
    })
    drop(operationId)
  } catch (error) {
    log.warn('Could not {how} op={operationId}; the question stays up: {error}', { how, operationId, error })
  } finally {
    answering = false
  }
}

/** "Move the rest" (or copy, delete, …): carries the operation on with the
 *  sources still there. Any outcome takes the question down: `not_asking` and
 *  `unknown_operation` mean the operation moved on without us. */
export async function continueSourcesGonePrompt(): Promise<void> {
  await answer('continue', continueWithoutGoneSources)
}

/** Ends the operation through the ordinary cancel, which also drops its parked
 *  question on the backend. */
export async function cancelSourcesGonePrompt(): Promise<void> {
  await answer('cancel', cancelOperation)
}

/**
 * Drops questions whose operation is no longer live. Exported so the pass is
 * callable straight from a test; the effect only decides when it runs.
 */
export function reconcileSourcesGonePrompts(rows: OperationRow[]): void {
  if (entries.length === 0) return
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local for one pass; nothing renders from it
  const live = new Set(rows.filter((r) => !isTerminalStatus(r.snapshot.status)).map((r) => r.snapshot.operationId))
  const gone: string[] = []
  let confirmed = false
  const next: Entry[] = []
  for (const entry of entries) {
    const id = entry.event.operationId
    if (live.has(id) && !entry.confirmedLive) {
      confirmed = true
      next.push({ ...entry, confirmedLive: true })
      continue
    }
    if (!live.has(id) && entry.confirmedLive) gone.push(id)
    next.push(entry)
  }
  if (confirmed) entries = next
  for (const id of gone) {
    log.info('Dropping the sources-gone question for {operationId}: the operation is gone', { operationId: id })
    drop(id)
  }
}

/** Starts listening. Idempotent; call {@link stopSourcesGoneHost} on teardown. */
export async function startSourcesGoneHost(): Promise<void> {
  if (unlisten ?? stopEffects) return
  stopEffects = $effect.root(() => {
    $effect(() => {
      reconcileSourcesGonePrompts(getMainWindowOperationRows())
    })
  })
  unlisten = await onWriteSourcesGone(handleSourcesGone)
}

/** Drops the listener, the effect, and every question. */
export function stopSourcesGoneHost(): void {
  unlisten?.()
  unlisten = null
  stopEffects?.()
  stopEffects = null
  entries = []
  shownId = null
  answering = false
  offerPending = false
}
