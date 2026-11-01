/**
 * Why a key's action left rows as they were, in words. The backend decides (`set_sync_actions` answers every row it
 * left alone, grouped by `RowSkipReason`, plus the hidden ones); this only says it, so nothing is ever silent.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { joinSentences } from '#lib/intl/sentence-join.js'
import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
import type { OverrideRefusal, RowSkipReason, Side, SkippedRows } from '#lib/tauri-commands/sync.js'
import { actionLabel } from './row-label'

const bySide = (side: Side, left: MessageKey, right: MessageKey): MessageKey => (side === 'left' ? left : right)

function skippedKey(reason: RowSkipReason): MessageKey {
  switch (reason.type) {
    case 'illegalForMode':
      return 'sync.skipped.illegalForMode'
    case 'illegalForCategory':
      return 'sync.skipped.illegalForCategory'
    case 'nothingToCopy':
      return bySide(reason.side, 'sync.skipped.nothingOnLeft', 'sync.skipped.nothingOnRight')
    case 'sideReadOnly':
      return bySide(reason.side, 'sync.skipped.sideReadOnlyLeft', 'sync.skipped.sideReadOnlyRight')
    case 'cantReverse':
      return 'sync.skipped.cantReverse'
    case 'nothingToToggle':
      return 'sync.skipped.nothingToToggle'
    default: {
      const unreachable: never = reason
      return unreachable
    }
  }
}

/** "3 rows weren't changed: the right side takes no changes." */
export function skippedText(rows: SkippedRows): string {
  const { reason, count } = rows
  const action = reason.type === 'illegalForMode' || reason.type === 'illegalForCategory' ? reason.action : null
  return tString(skippedKey(reason), {
    count,
    countText: formatNumber(count),
    action: action === null ? '' : actionLabel(action),
  })
}

/** Everything a key left alone, one sentence per reason, the hidden rows last; `null` when it changed every row. */
export function actionsNotice(skipped: SkippedRows[], hidden: number): string | null {
  const sentences = skipped.map(skippedText)
  if (hidden > 0) sentences.push(tString('sync.skipped.hidden', { count: hidden, countText: formatNumber(hidden) }))
  return sentences.length > 0 ? joinSentences(sentences) : null
}

/** A typed refusal of one row's change, in words. */
export function overrideRefusalText(refusal: OverrideRefusal): string {
  switch (refusal.type) {
    case 'noSuchItem':
      return tString('sync.error.noSuchItem')
    case 'illegalForMode':
      return tString('sync.refusal.illegalForMode', { action: actionLabel(refusal.action) })
    case 'illegalForCategory':
      return tString('sync.refusal.illegalForCategory', { action: actionLabel(refusal.action) })
    case 'nothingToCopy':
      return tString(bySide(refusal.side, 'sync.refusal.nothingOnLeft', 'sync.refusal.nothingOnRight'))
    case 'sideReadOnly':
      return tString(bySide(refusal.side, 'sync.refusal.sideReadOnlyLeft', 'sync.refusal.sideReadOnlyRight'))
    default: {
      const unreachable: never = refusal
      return unreachable
    }
  }
}
