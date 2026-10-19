/**
 * The progress dialog's one reversal decision: whether Rollback works right
 * now, what it promises, and the question asked before it happens.
 *
 * It's a controller rather than a component because the decision surfaces in
 * two places that can't share a subtree: the Rollback button sits in the
 * dialog's button row (`TransferRollbackControls.svelte`), while the
 * confirmation stacks OVER the dialog and must stay reachable from the conflict
 * body too, which replaces that row. `../DETAILS.md` § "Rollback asks first".
 */

import type { OpKind, RollbackSupport, WriteOperationPhase } from '#lib/ipc/bindings.js'
import { tString } from '#lib/intl/messages.svelte.js'
import {
  inFlightRollbackTooltipKey,
  inFlightRollbackVariant,
  reversalWindowClosed,
  rollbackBlockedTooltipKey,
  type RollbackConfirmVariant,
} from '../reversal-wording'

export interface TransferRollbackInputs {
  opKind: () => OpKind
  phase: () => WriteOperationPhase | null
  /** Whether the operation's strategy can reverse, and if not, why. The
   *  registry row's verdict, or the dialog's same-volume-move rule for the
   *  frames before it lands; `null` while neither knows, which offers Rollback. */
  rollbackSupport: () => RollbackSupport | null
  operationSettled: () => boolean
  /** Runs the confirmed rollback (`progress.handleCancel(true)`). */
  rollBack: () => void
}

export interface TransferRollback {
  /** What rolling THIS operation back does to the files: a copy's reversal
   *  deletes what it wrote, a move's carries it back. ❌ Never a fixed
   *  `stopAndDelete`, which pushes people off a move's harmless reversal. */
  readonly variant: RollbackConfirmVariant
  /** What the live button promises, the whole difference from Cancel. */
  readonly liveTooltip: string
  /** Why Rollback is switched off right now, or `null` while it really works. */
  readonly blockedTooltip: string | null
  /** The confirmation is up. Withdrawn once the operation settles (nothing
   *  left to undo) and the moment Rollback becomes blocked. */
  readonly confirming: boolean
  /** Asks the question, unless Rollback is blocked. Both entry points (the
   *  button and the conflict body) come through here. */
  request: () => void
  confirm: () => void
  dismiss: () => void
}

export function createTransferRollback(inputs: TransferRollbackInputs): TransferRollback {
  let asked = $state(false)
  const variant = $derived(inFlightRollbackVariant(inputs.opKind()))
  const liveTooltip = $derived(tString(inFlightRollbackTooltipKey(variant)))
  /** Two reasons, and they answer different halves of the question:
   *
   *  - the operation's STRATEGY can't reverse at all (`rollbackSupport`, worded
   *    per reason: a move between volumes, one within a volume, a zip rewrite),
   *  - or it could, and the moment has passed: a move between filesystems on its
   *    source-deletion phase has already landed every file
   *    (`../reversal-wording.ts`).
   *
   *  Either way the plain Cancel stays live and stays accurate, which is what
   *  every one of these tooltips points at. */
  const blockedTooltip = $derived.by(() => {
    const support = inputs.rollbackSupport()
    const strategyKey = support === null ? null : rollbackBlockedTooltipKey(support)
    if (strategyKey !== null) return tString(strategyKey)
    if (reversalWindowClosed(inputs.opKind(), inputs.phase()))
      return tString('fileOperations.transferProgress.rollbackAlreadyLandedTooltip')
    return null
  })

  return {
    get variant() {
      return variant
    },
    get liveTooltip() {
      return liveTooltip
    },
    get blockedTooltip() {
      return blockedTooltip
    },
    get confirming() {
      return asked && !inputs.operationSettled() && blockedTooltip === null
    },
    request() {
      if (blockedTooltip === null) asked = true
    },
    confirm() {
      asked = false
      inputs.rollBack()
    },
    dismiss() {
      asked = false
    },
  }
}
