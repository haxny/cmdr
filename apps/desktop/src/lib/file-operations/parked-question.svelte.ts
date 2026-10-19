/**
 * The machinery shared by the main window's hosts for a question an operation
 * PARKS on until somebody answers it: "sources left while it queued"
 * (`sources-gone.svelte.ts`) and "these items are Locked" (`locked-sources.svelte.ts`).
 * Each host keeps what's its own (its event, its toasts, its answer commands);
 * this owns the rest:
 *
 * - **One question per operation, one on screen at a time**, oldest first: a
 *   repeat from the same operation replaces its question rather than asking twice.
 * - **It waits for the window to be free** (`createWhenDialogsClear`), the
 *   late-dialog rule a background job's prompt follows, by the host's own
 *   `isBusy`; while held, the host's `onHeld` says so (a quiet toast).
 * - **An answer that never landed leaves the question up**: the operation is
 *   still waiting for it. Any other outcome takes it down.
 * - **The operation ends some other way** (Cancel in the queue window, a
 *   teardown): its question goes, once the operation has been SEEN live, since
 *   until then its absence from the rows means "hasn't arrived yet".
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import type { OperationSnapshot } from '#lib/tauri-commands/index.js'
import type { getAppLogger } from '#lib/logging/logger.js'
import { createWhenDialogsClear } from '#lib/file-explorer/pane/when-dialogs-clear.svelte.js'
import { getMainWindowOperationRows } from './queue/main-window-operations.svelte'
import { isTerminalStatus, type OperationRow } from './queue/operations-store.svelte'

/** What every parked question names: the operation that asks it. */
export interface ParkedQuestionEvent {
  operationId: string
}

/** What a prompt renders. */
export interface ParkedQuestionPrompt<E extends ParkedQuestionEvent> {
  operationId: string
  event: E
  /** The operation's row, for naming which operation is asking. `null` only in
   *  the sliver before its snapshot lands. */
  snapshot: OperationSnapshot | null
}

interface Entry<E> {
  event: E
  /** True once the operation has been seen live in a snapshot. */
  confirmedLive: boolean
}

export interface ParkedQuestionOptions<E extends ParkedQuestionEvent> {
  log: ReturnType<typeof getAppLogger>
  /** Names the question in log lines, like "sources-gone". */
  label: string
  /** Whether the window is too busy to ask `asking`'s question now. */
  isBusy: (asking: E | undefined) => boolean
  /** The oldest question has to wait for the window to free up. */
  onHeld: (event: E) => void
}

export function createParkedQuestionHost<E extends ParkedQuestionEvent>(options: ParkedQuestionOptions<E>) {
  const { log, label } = options
  /** Unanswered questions, oldest first. */
  let entries = $state<Entry<E>[]>([])
  /** The operation whose question is on screen, or `null`. */
  let shownId = $state<string | null>(null)
  /** An answer is in flight; the buttons disable. */
  let answering = $state(false)
  /** A question is already waiting for the window to free up. */
  let offerPending = false
  let unlisten: UnlistenFn | null = null
  let stopEffects: (() => void) | null = null

  const whenDialogsClear = createWhenDialogsClear(() => options.isBusy(entries[0]?.event))

  function offerNext(): void {
    if (shownId !== null || offerPending || entries.length === 0) return
    offerPending = true
    const askedNow = whenDialogsClear.run(() => {
      offerPending = false
      shownId = entries[0]?.event.operationId ?? null
    })
    if (!askedNow) options.onHeld(entries[0].event)
  }

  function drop(operationId: string): void {
    entries = entries.filter((e) => e.event.operationId !== operationId)
    if (shownId === operationId) shownId = null
    offerNext()
  }

  /** Drops questions whose operation is no longer live. */
  function reconcile(rows: OperationRow[]): void {
    if (entries.length === 0) return
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- transient local for one pass; nothing renders from it
    const live = new Set(rows.filter((r) => !isTerminalStatus(r.snapshot.status)).map((r) => r.snapshot.operationId))
    const gone: string[] = []
    let confirmed = false
    const next: Entry<E>[] = []
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
      log.info('Dropping the {label} question for {operationId}: the operation is gone', { label, operationId: id })
      drop(id)
    }
  }

  return {
    /** The question on screen, or `null`. Reactive. */
    prompt(): ParkedQuestionPrompt<E> | null {
      if (shownId === null) return null
      const entry = entries.find((e) => e.event.operationId === shownId)
      if (!entry) return null
      const snapshot = getMainWindowOperationRows().find((r) => r.snapshot.operationId === shownId)?.snapshot ?? null
      return { operationId: shownId, event: entry.event, snapshot }
    },

    /** An answer is in flight. Reactive. */
    isAnswering(): boolean {
      return answering
    },

    /** Queues `event`'s question, replacing an earlier one from the same operation. */
    ask(event: E): void {
      const others = entries.filter((e) => e.event.operationId !== event.operationId)
      entries = [...others, { event, confirmedLive: false }]
      offerNext()
    },

    /** Runs one answer for the question on screen. */
    async answer(how: string, send: (operationId: string) => Promise<unknown>): Promise<void> {
      const operationId = shownId
      if (operationId === null || answering) return
      answering = true
      try {
        const outcome = await send(operationId)
        log.info('The {label} question for op={operationId} was answered with {how}: {outcome}', {
          label,
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
    },

    reconcile,

    /** Starts listening through `listen`, and reconciling against the rows.
     *  Idempotent. */
    async start(listen: (handle: (event: E) => void) => Promise<UnlistenFn>, handle: (event: E) => void) {
      if (unlisten ?? stopEffects) return
      stopEffects = $effect.root(() => {
        $effect(() => {
          reconcile(getMainWindowOperationRows())
        })
      })
      unlisten = await listen(handle)
    },

    /** Drops the listener, the effect, and every question. */
    stop(): void {
      unlisten?.()
      unlisten = null
      stopEffects?.()
      stopEffects = null
      entries = []
      shownId = null
      answering = false
      offerPending = false
    },
  }
}
