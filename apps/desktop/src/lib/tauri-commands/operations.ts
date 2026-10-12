// Operation-manager IPC: the queue window's view of every running/queued
// copy, move, delete, and trash operation, plus the pause/resume/cancel
// controls. The thin `operations-changed` event carries membership + lifecycle
// status; live per-row bars come from the separate `write-progress` stream
// (see `onWriteProgress` in `write-operations.ts`).

import type { UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '#lib/ipc/bindings.js'
import { throwIpcError } from './ipc-types'
import type {
  OperationSnapshot,
  OperationsChanged,
  ProgressAxis,
  QueueMoveDirection,
  QueueMoveOutcome,
  RetryOutcome,
  SourcesGoneAnswerOutcome,
  TransferTreeAncestor,
  TransferTreeRow,
  TransferTreeView,
  TransferTreeViewError,
  TransferTreeViewRequest,
  TreeRowKind,
  TreeRowState,
  TreeWindow,
  WriteSourcesGoneEvent,
} from '#lib/ipc/bindings.js'

export type {
  OperationSnapshot,
  OperationsChanged,
  ProgressAxis,
  QueueMoveDirection,
  QueueMoveOutcome,
  RetryOutcome,
  SourcesGoneAnswerOutcome,
  TransferTreeAncestor,
  TransferTreeRow,
  TransferTreeView,
  TransferTreeViewError,
  TransferTreeViewRequest,
  TreeRowKind,
  TreeRowState,
  TreeWindow,
  WriteSourcesGoneEvent,
}

/** Snapshot of every operation the manager currently tracks (queued, running,
 *  paused, and recently-terminal until it's pruned). */
export async function listOperations(): Promise<OperationSnapshot[]> {
  return commands.listOperations()
}

/** A window of an operation's live transfer tree, or the backend's typed reason
 *  there's none. */
export type TransferTreeAnswer = TransferTreeView | { refusal: TransferTreeViewError }

/** One window of an operation's live transfer tree: call it on mount and on each
 *  `write-progress` tick of the operation, with the view's own hand toggles and
 *  scroll position. `ready: false` while the operation scans. A refusal is an
 *  ordinary answer, never thrown: an operation that settled between two ticks
 *  answers `unknown_operation`, and a delete answers `not_a_transfer`. */
export async function getTransferTreeView(
  operationId: string,
  request: TransferTreeViewRequest,
): Promise<TransferTreeAnswer> {
  const res = await commands.getTransferTreeView(operationId, request)
  return res.status === 'ok' ? res.data : { refusal: res.error }
}

/** Cancel one operation, keeping already-copied files (rollback = false). A
 *  queued op is dropped before it spawns; a running/paused op stops and keeps
 *  partials. */
export async function cancelOperation(operationId: string): Promise<void> {
  await commands.cancelOperation(operationId)
}

/** A queued operation found some of its sources gone when its turn came. With
 *  `next: 'awaiting_answer'` it waits for `continueWithoutGoneSources` or a
 *  cancel; with `'nothing_left'` it already completed and this only says why. */
export async function onWriteSourcesGone(callback: (event: WriteSourcesGoneEvent) => void): Promise<UnlistenFn> {
  return events.writeSourcesGone.listen((event) => {
    callback(event.payload)
  })
}

/** Carries a queued operation on with the sources that are still there. The
 *  outcome says whether this answer is the one that reached it. */
export async function continueWithoutGoneSources(operationId: string): Promise<SourcesGoneAnswerOutcome> {
  return commands.continueWithoutGoneSources(operationId)
}

/** Cancel several operations at once (the "Cancel selected" action). Same
 *  keep-partials semantics as `cancelOperation`. */
export async function cancelOperations(operationIds: string[]): Promise<void> {
  await commands.cancelOperations(operationIds)
}

/** Pause one running operation in place. It keeps its lane slot and can be
 *  resumed; its snapshot status flips to `paused`.
 *
 *  The command answers with a `PauseOutcome`, dropped here on purpose: every
 *  frontend surface renders the live status from `operations-changed`, so none
 *  of them has to trust a return value. The MCP `queue` tool is the consumer
 *  that reads it, since an agent has nothing else to go on. */
export async function pauseOperation(operationId: string): Promise<void> {
  await commands.pauseOperation(operationId)
}

/** Resume one paused operation. Its `PauseOutcome` is dropped for the same
 *  reason as `pauseOperation`'s. */
export async function resumeOperation(operationId: string): Promise<void> {
  await commands.resumeOperation(operationId)
}

/** Pause every running operation. Its `PauseAllOutcome` counts are dropped for
 *  the same reason `pauseOperation`'s single outcome is: the queue window renders
 *  the live statuses. The MCP `queue` tool reads them. */
export async function pauseAll(): Promise<void> {
  await commands.pauseAll()
}

/** Resume every paused operation. Its counts are dropped like `pauseAll`'s. */
export async function resumeAll(): Promise<void> {
  await commands.resumeAll()
}

/** Move a waiting operation one place earlier or later in line (the queue
 *  window's ⌥↑ / ⌥↓). Running and paused operations keep their place. The
 *  outcome is the operation's place after the request, which the window
 *  announces: it's the one answer `operations-changed` can't give a screen
 *  reader, since the list re-renders silently. */
export async function moveQueuedOperation(
  operationId: string,
  direction: QueueMoveDirection,
): Promise<QueueMoveOutcome> {
  return commands.moveQueuedOperation(operationId, direction)
}

/** Drop one retained failure from the snapshot. Dismissal is always explicit —
 *  a failed row waits until someone reads it, however long that takes. */
export async function dismissFailedOperation(operationId: string): Promise<void> {
  await commands.dismissFailedOperation(operationId)
}

/** Start a retained failure again as a new operation (the queue row's Retry).
 *  The backend kept how it started (`routing/retry.rs`), so this works from any
 *  window. A started retry replaces the failed row; a refused one (the volume is
 *  gone) rejects with the typed `WriteOperationError` and leaves the row. */
export async function retryFailedOperation(operationId: string): Promise<RetryOutcome> {
  const res = await commands.retryFailedOperation(operationId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Edit > Select all (⌘A) while the queue window is focused. The menu
 *  accelerator fires before the page's keydown sees ⌘A, so this is how the
 *  window learns of it (`menu_handlers.rs` routes it here instead of the native
 *  `selectAll:`). Returns an `UnlistenFn`. */
export async function onQueueSelectAll(callback: () => void): Promise<UnlistenFn> {
  return events.queueSelectAll.listen(() => {
    callback()
  })
}

/** Drop every retained failure (the queue toolbar's "Dismiss all"). */
export async function dismissAllFailedOperations(): Promise<void> {
  await commands.dismissAllFailedOperations()
}

/** Subscribe to the thin registry snapshot (membership + lifecycle status). The
 *  queue window reduces this into its row set. Returns an `UnlistenFn`; call it
 *  on teardown or you leak the listener. */
export async function onOperationsChanged(callback: (event: OperationsChanged) => void): Promise<UnlistenFn> {
  return events.operationsChanged.listen((event) => {
    callback(event.payload)
  })
}
