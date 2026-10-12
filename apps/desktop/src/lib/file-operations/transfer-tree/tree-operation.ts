/**
 * When a surface shows the live tree: one rule for the progress dialog and the
 * queue window's rows, so the two can't disagree about which operations have one.
 */

import type { WriteOperationPhase, WriteOperationType } from '#lib/ipc/bindings.js'
import type { TransferTreeOperation } from './row-format'

/**
 * The operation a tree shows for, or `null` where the surface keeps its plain
 * readout. A copy, move, or compress has a tree once its scan is over (before
 * then there's none to show: `phase` is `scanning`, or `null` in a view that
 * hasn't heard a tick yet). Delete, trash, and an edit inside an existing
 * archive never build one. ❌ Not while a reversal runs, either an in-flight
 * rollback or one launched from history: its rows would still say "done" about
 * files that are going away.
 */
export function treeOperationFor(input: {
  operationType: WriteOperationType
  phase: WriteOperationPhase | null
  reversing: boolean
}): TransferTreeOperation | null {
  const { operationType, phase, reversing } = input
  if (operationType !== 'copy' && operationType !== 'move' && operationType !== 'compress') return null
  if (phase === null || phase === 'scanning' || reversing) return null
  return operationType
}
