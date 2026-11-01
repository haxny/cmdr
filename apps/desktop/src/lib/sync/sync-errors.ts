/**
 * Every refusal the backend types (`SyncError`) in words, by variant, never by message. An override refusal speaks
 * as the row refusal it is (`action-refusal.ts`).
 */
import { getMessage } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { OverrideRefusal, Side, SyncMaskError } from '#lib/tauri-commands/index.js'
import { rowRefusalText } from './action-refusal'
import type { SyncProblem } from './sync-session.svelte'

const bySide = (side: Side, left: MessageKey, right: MessageKey): string => getMessage(side === 'left' ? left : right)

function maskText(error: SyncMaskError): string {
  switch (error.type) {
    case 'emptyMask':
      return getMessage('sync.error.maskEmpty')
    case 'unclosedQuote':
      return getMessage('sync.error.maskQuote')
    case 'secondBar':
      return getMessage('sync.error.maskSecondBar')
  }
}

function overrideText(refusal: OverrideRefusal): string {
  switch (refusal.type) {
    case 'noSuchItem':
      return getMessage('sync.error.noSuchSession')
    case 'illegalForMode':
    case 'illegalForCategory':
      return rowRefusalText(refusal)
    case 'sideReadOnly':
      return bySide(refusal.side, 'sync.refusal.sideReadOnlyLeft', 'sync.refusal.sideReadOnlyRight')
    case 'nothingToCopy':
      return rowRefusalText({ type: 'nothingToCopy', side: refusal.side })
  }
}

/** The variants whose words depend on nothing but the variant. */
const PLAIN: Partial<Record<SyncProblem['type'], MessageKey>> = {
  noSuchSession: 'sync.error.noSuchSession',
  notCompared: 'sync.error.notCompared',
  keepFolderUnreadable: 'sync.error.keepFolderUnreadable',
  keep: 'sync.error.keep',
  compareRunning: 'sync.error.compareRunning',
  runInProgress: 'sync.error.runInProgress',
  planChanged: 'sync.error.planChanged',
  staleCompare: 'sync.error.staleCompare',
  notContentComparable: 'sync.error.notContentComparable',
  notInstalled: 'sync.error.notInstalled',
  unexpected: 'sync.error.unexpected',
}

/** The variants that name a side. */
function sidedText(problem: SyncProblem): string | null {
  switch (problem.type) {
    case 'volumeNotConnected':
      return bySide(problem.side, 'sync.error.volumeNotConnectedLeft', 'sync.error.volumeNotConnectedRight')
    case 'rootUnreachable':
      return bySide(problem.side, 'sync.error.rootUnreachableLeft', 'sync.error.rootUnreachableRight')
    case 'badPath':
      return bySide(problem.side, 'sync.error.badPathLeft', 'sync.error.badPathRight')
    case 'sideReadOnly':
      return bySide(problem.side, 'sync.error.sideReadOnlyLeft', 'sync.error.sideReadOnlyRight')
    case 'staleSelection':
      return bySide(problem.side, 'sync.error.staleSelectionLeft', 'sync.error.staleSelectionRight')
    default:
      return null
  }
}

/** A refusal in words, by its variant. */
export function syncProblemText(problem: SyncProblem): string {
  const plain = PLAIN[problem.type]
  if (plain) return getMessage(plain)
  const sided = sidedText(problem)
  if (sided !== null) return sided
  switch (problem.type) {
    case 'rootsConflict':
      return problem.conflict.type === 'sameFolder'
        ? getMessage('sync.error.sameFolder')
        : bySide(problem.conflict.outer, 'sync.error.nestedLeft', 'sync.error.nestedRight')
    case 'badMask':
      return maskText(problem.error)
    case 'badSettings':
      return problem.problem.type === 'badMask' ? maskText(problem.problem.error) : getMessage('sync.error.badSettings')
    case 'override':
      return overrideText(problem.refusal)
    default:
      return getMessage('sync.error.unexpected')
  }
}
