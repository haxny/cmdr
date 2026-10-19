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
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  answerLockedSources,
  cancelOperation,
  onWriteLockedSources,
  type LockedSourcesAnswer,
  type OperationSnapshot,
  type WriteLockedSourcesEvent,
} from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { addToast } from '#lib/ui/toast/index.js'
import { isAnySoftDialogOpenBesidesAll } from '#lib/ui/open-dialogs.svelte.js'
import { createWhenDialogsClear } from '#lib/file-explorer/pane/when-dialogs-clear.svelte.js'
import { getForegroundOperationId } from './foreground-operation.svelte'
import { getMainWindowOperationRows } from './queue/main-window-operations.svelte'
import { isTerminalStatus, type OperationRow } from './queue/operations-store.svelte'

const log = getAppLogger('locked-sources')

/** A question waiting for an answer. */
interface Entry {
  event: WriteLockedSourcesEvent
  /** True once the operation has been seen live in a snapshot. Until then its
   *  absence from the rows means "hasn't arrived yet", not "gone". */
  confirmedLive: boolean
}

/** What the prompt renders. */
export interface LockedSourcesPrompt {
  operationId: string
  event: WriteLockedSourcesEvent
  /** The operation's row, for naming which move is asking. `null` only in the
   *  sliver before its snapshot lands. */
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

/** Busy while any OTHER dialog is on screen, the asking move's own progress
 *  dialog aside. This prompt's own previous question is no reason to hold the
 *  next: `shownId` already keeps it to one at a time. */
function isBusy(): boolean {
  const asking = entries[0]?.event.operationId
  if (asking !== undefined && getForegroundOperationId() === asking) {
    return isAnySoftDialogOpenBesidesAll(['locked-sources', 'transfer-progress'])
  }
  return isAnySoftDialogOpenBesidesAll(['locked-sources'])
}

const whenDialogsClear = createWhenDialogsClear(isBusy)

/** The question on screen, or `null`. Reactive. */
export function getLockedSourcesPrompt(): LockedSourcesPrompt | null {
  if (shownId === null) return null
  const entry = entries.find((e) => e.event.operationId === shownId)
  if (!entry) return null
  const snapshot = getMainWindowOperationRows().find((r) => r.snapshot.operationId === shownId)?.snapshot ?? null
  return { operationId: shownId, event: entry.event, snapshot }
}

/** An answer is in flight. Reactive. */
export function isAnsweringLockedSources(): boolean {
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
    log.info('A move is waiting to ask about its Locked items; asking once the open dialog closes')
    addToast(tString('fileOperations.lockedSources.heldToast'), { level: 'info', timeoutMs: 10000 })
  }
}

function handleLockedSources(event: WriteLockedSourcesEvent): void {
  log.info('Op={operationId} found {count} Locked item(s); asking', {
    operationId: event.operationId,
    count: event.lockedCount,
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

/** Runs one answer. A call that never landed leaves the question up: the move
 *  is still waiting for it. */
async function answer(how: string, send: (operationId: string) => Promise<unknown>): Promise<void> {
  const operationId = shownId
  if (operationId === null || answering) return
  answering = true
  try {
    const outcome = await send(operationId)
    log.info('Locked-items question for op={operationId} answered with {how}: {outcome}', {
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

function sendAnswer(choice: LockedSourcesAnswer): (operationId: string) => Promise<unknown> {
  return (operationId) => answerLockedSources(operationId, choice)
}

/** "Move anyway": the move unlocks each Locked item right before it takes it,
 *  and locks it again where it lands. Any outcome takes the question down:
 *  `not_asking` and `unknown_operation` mean the move went on without us. */
export async function moveLockedSourcesAnyway(): Promise<void> {
  await answer('move anyway', sendAnswer('move_anyway'))
}

/** "Skip": the Locked items stay where they are, and the rest moves. */
export async function skipLockedSources(): Promise<void> {
  await answer('skip', sendAnswer('skip'))
}

/** Ends the move through the ordinary cancel, which also drops its parked
 *  question on the backend. */
export async function cancelLockedSourcesPrompt(): Promise<void> {
  await answer('cancel', cancelOperation)
}

/**
 * Drops questions whose operation is no longer live. Exported so the pass is
 * callable straight from a test; the effect only decides when it runs.
 */
export function reconcileLockedSourcesPrompts(rows: OperationRow[]): void {
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
    log.info('Dropping the Locked-items question for {operationId}: the operation is gone', { operationId: id })
    drop(id)
  }
}

/** Starts listening. Idempotent; call {@link stopLockedSourcesHost} on teardown. */
export async function startLockedSourcesHost(): Promise<void> {
  if (unlisten ?? stopEffects) return
  stopEffects = $effect.root(() => {
    $effect(() => {
      reconcileLockedSourcesPrompts(getMainWindowOperationRows())
    })
  })
  unlisten = await onWriteLockedSources(handleLockedSources)
}

/** Drops the listener, the effect, and every question. */
export function stopLockedSourcesHost(): void {
  unlisten?.()
  unlisten = null
  stopEffects?.()
  stopEffects = null
  entries = []
  shownId = null
  answering = false
  offerPending = false
}
