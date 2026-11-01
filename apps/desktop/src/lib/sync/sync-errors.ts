/**
 * Every refusal the backend types (`SyncError`) in words, by variant, never by message. The table is keyed by every
 * variant, so a new one doesn't compile until it has words.
 */
import { getMessage } from '#lib/intl/messages.svelte.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { Side, SyncMaskError } from '#lib/tauri-commands/sync.js'
import { overrideRefusalText } from './action-refusal'
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
    default: {
      const unreachable: never = error
      return unreachable
    }
  }
}

type ProblemText = { [K in SyncProblem['type']]: (problem: Extract<SyncProblem, { type: K }>) => string }

const PROBLEM_TEXT: ProblemText = {
  noSuchSession: () => getMessage('sync.error.noSuchSession'),
  notCompared: () => getMessage('sync.error.notCompared'),
  volumeNotConnected: (p) => bySide(p.side, 'sync.error.volumeNotConnectedLeft', 'sync.error.volumeNotConnectedRight'),
  rootUnreachable: (p) => bySide(p.side, 'sync.error.rootUnreachableLeft', 'sync.error.rootUnreachableRight'),
  badPath: (p) => bySide(p.side, 'sync.error.badPathLeft', 'sync.error.badPathRight'),
  rootsConflict: (p) =>
    p.conflict.type === 'sameFolder'
      ? getMessage('sync.error.sameFolder')
      : bySide(p.conflict.outer, 'sync.error.nestedLeft', 'sync.error.nestedRight'),
  badMask: (p) => maskText(p.error),
  badSettings: (p) => (p.problem.type === 'badMask' ? maskText(p.problem.error) : getMessage('sync.error.badSettings')),
  keepFolderUnreadable: () => getMessage('sync.error.keepFolderUnreadable'),
  keep: () => getMessage('sync.error.keep'),
  sideReadOnly: (p) => bySide(p.side, 'sync.error.sideReadOnlyLeft', 'sync.error.sideReadOnlyRight'),
  staleSelection: (p) => bySide(p.side, 'sync.error.staleSelectionLeft', 'sync.error.staleSelectionRight'),
  compareRunning: () => getMessage('sync.error.compareRunning'),
  runInProgress: () => getMessage('sync.error.runInProgress'),
  planChanged: () => getMessage('sync.error.planChanged'),
  staleCompare: () => getMessage('sync.error.staleCompare'),
  override: (p) => overrideRefusalText(p.refusal),
  notContentComparable: () => getMessage('sync.error.notContentComparable'),
  notInstalled: () => getMessage('sync.error.notInstalled'),
  unexpected: () => getMessage('sync.error.unexpected'),
}

/** A refusal in words, by its variant. */
export function syncProblemText(problem: SyncProblem): string {
  const text = PROBLEM_TEXT[problem.type] as (problem: SyncProblem) => string
  return text(problem)
}
