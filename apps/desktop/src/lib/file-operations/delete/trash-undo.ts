/**
 * Undoing a trash: run the rollback engine over the operation and say what came
 * back, honestly.
 *
 * The engine already knows how to reverse a trash — a trash row records the OS's
 * own in-trash location, and its inverse is a restore-move back to the source
 * (`operation_log/rollback.rs`). This is the surface that offers it the moment it
 * matters, instead of leaving it in the operation log where nobody looks three
 * seconds after a mis-keyed F8.
 *
 * The composition is pure and lives apart from the toast so the honesty rule is
 * testable on its own: anything left behind outranks what came back. A restore
 * that put nine of ten files back must never read as a clean success, because the
 * tenth is still in the trash and only the user can decide that's fine.
 */

import { undoOperations, type UndoReport } from '#lib/tauri-commands/index.js'
import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { addToast, dismissToast } from '#lib/ui/toast/index.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('fileOperations')

/**
 * Dedup id for the undo's own toast. Stable, so hitting Undo twice can't stack
 * two conversations about the same restore.
 */
const TRASH_UNDO_TOAST_ID = 'trash-undo'

/** The dedup id of a trash's own completion toast (Undo / "Go to trash"), so Edit > Undo trash can take it down. */
export function trashCompleteToastId(operationId: string): string {
  return `trash-complete-${operationId}`
}

/**
 * The newest trash this session that nobody has undone yet: what Edit > Undo trash
 * (`edit.undoTrash`) reverses. The toast's Undo button is mouse-first and gone after ten
 * seconds; this keeps the same action a menu item and a palette command away for as long
 * as it's the last thing trashed. Session-only on purpose: an "Undo" that reached back to
 * a trash from yesterday would surprise more than it helps, and the operation log already
 * rolls back any trash in the journal.
 */
let lastTrashOperationId: string | null = null

/** Records a finished trash as the one Edit > Undo trash reverses. */
export function rememberTrashForUndo(operationId: string): void {
  lastTrashOperationId = operationId
}

/** What a finished trash undo amounts to, before it becomes a sentence. */
export type TrashUndoOutcome =
  /** Every item is back where it was. */
  | { status: 'restored'; restored: number }
  /** Some came back, some stayed in the trash. */
  | { status: 'partial'; restored: number; skipped: number }
  /** Nothing was reversed and nothing was even attempted. */
  | { status: 'unavailable' }

/**
 * Read a finished undo as an outcome.
 *
 * A refused operation carries no per-item numbers at all (the whole thing was
 * turned down: already undone, or a volume it needs is gone), so it can't be
 * folded into `skipped` without understating what was missed. It lands in
 * `partial` when anything came back and `unavailable` when nothing did.
 */
export function trashUndoOutcome(report: UndoReport): TrashUndoOutcome {
  const refused = report.operations.some((operation) => operation.refusal !== null)
  if (report.restored === 0 && report.skipped === 0) return { status: 'unavailable' }
  if (report.skipped > 0 || refused) {
    return { status: 'partial', restored: report.restored, skipped: report.skipped }
  }
  return { status: 'restored', restored: report.restored }
}

/** The one line the toast shows once the undo settles. */
export function trashUndoMessage(outcome: TrashUndoOutcome): string {
  switch (outcome.status) {
    case 'restored':
      return tString('fileOperations.trash.undone', {
        countText: formatNumber(outcome.restored),
        count: outcome.restored,
      })
    case 'partial':
      return tString('fileOperations.trash.undonePartial', {
        restoredText: formatNumber(outcome.restored),
        restored: outcome.restored,
        skippedText: formatNumber(outcome.skipped),
        skipped: outcome.skipped,
      })
    case 'unavailable':
      return tString('fileOperations.trash.undoUnavailable')
  }
}

/** A partial result is not a success; the toast colours itself from this. */
export function trashUndoLevel(outcome: TrashUndoOutcome): 'success' | 'info' {
  return outcome.status === 'restored' ? 'success' : 'info'
}

/**
 * Run the undo and report it, start to finish.
 *
 * A restore is a queued operation, so it waits out anything already working the
 * same volume and can take a while. That's why it gets a PERSISTENT progress
 * toast rather than a transient one: a restore that outlives a 7-second timeout
 * must not leave the user with no sign that anything is happening. `addToast`
 * replaces content and level in place but never dismissal, so the result arrives
 * as a fresh transient toast rather than a replacement.
 */
export async function runTrashUndo(operationId: string): Promise<void> {
  // Whatever the outcome, this trash has had its undo: a second one could only be refused.
  if (lastTrashOperationId === operationId) lastTrashOperationId = null
  addToast(tString('fileOperations.trash.undoing'), {
    id: TRASH_UNDO_TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
  })

  try {
    const report = await undoOperations([operationId])
    const outcome = trashUndoOutcome(report)
    log.info('Trash undo for {operationId} finished: {status}', { operationId, status: outcome.status })
    dismissToast(TRASH_UNDO_TOAST_ID)
    addToast(trashUndoMessage(outcome), { level: trashUndoLevel(outcome), timeoutMs: 7000 })
  } catch (error) {
    log.warn('Trash undo for {operationId} did not run: {error}', { operationId, error: String(error) })
    dismissToast(TRASH_UNDO_TOAST_ID)
    addToast(tString('fileOperations.trash.undoUnavailable'), { level: 'info', timeoutMs: 7000 })
  }
}

/**
 * Edit > Undo trash: undo the newest trash not undone yet, taking its completion toast
 * down so its Undo can't run a second time. Says so when there's nothing to undo.
 */
export async function undoLastTrash(): Promise<void> {
  const operationId = lastTrashOperationId
  if (operationId === null) {
    addToast(tString('fileOperations.trash.nothingToUndo'), { level: 'info', id: TRASH_UNDO_TOAST_ID })
    return
  }
  dismissToast(trashCompleteToastId(operationId))
  await runTrashUndo(operationId)
}
