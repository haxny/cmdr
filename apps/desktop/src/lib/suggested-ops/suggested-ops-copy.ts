/**
 * The words and facts a group row reads out: which action the button names, why an approval
 * can or can't be taken back (or can't run at all), and what a file's size and date say now against what they said
 * when the AI suggested it.
 *
 * Pure, so the copy each verb gets and the comparison each row makes are pinned by tests rather
 * than by eyeballing a dialog.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { ProposalVerb, Reversibility, RunBlocker } from '#lib/ipc/bindings.js'
import type { SuggestedOpView } from '#lib/tauri-commands/index.js'
import type { LiveFact } from './suggested-ops-trigger.svelte'

/** The primary button for each verb, taking `{count}`. Named per verb so the button says what
 *  happens ("Move 4 files"), and stays the same whether or not the list is scrolled. */
export function approveKey(verb: ProposalVerb): MessageKey {
  switch (verb) {
    case 'move':
      return 'suggestedOps.approve.move'
    case 'copy':
      return 'suggestedOps.approve.copy'
    case 'trash':
      return 'suggestedOps.approve.trash'
    case 'delete':
      return 'suggestedOps.approve.delete'
    case 'rename':
      return 'suggestedOps.approve.rename'
    case 'compress':
      return 'suggestedOps.approve.compress'
    case 'extract':
      return 'suggestedOps.approve.extract'
  }
}

/**
 * The reversibility pill, saying WHY, taking `{count}`.
 *
 * The verb picks the reason: the same `restoreMove` means "it goes to the Trash" for a trash
 * group and "Cmdr can move them back" for a move.
 */
export function reversibilityKey(verb: ProposalVerb, reversible: Reversibility): MessageKey {
  switch (reversible) {
    case 'restoreMove':
      if (verb === 'trash') return 'suggestedOps.reversible.trash'
      if (verb === 'rename') return 'suggestedOps.reversible.renameBack'
      return 'suggestedOps.reversible.moveBack'
    case 'deleteWhatWasWritten':
      if (verb === 'compress') return 'suggestedOps.reversible.removeArchive'
      if (verb === 'extract') return 'suggestedOps.reversible.removeExtracted'
      return 'suggestedOps.reversible.removeCopies'
    case 'irreversible':
      if (verb === 'delete') return 'suggestedOps.irreversible.delete'
      if (verb === 'compress') return 'suggestedOps.irreversible.overwrite'
      return 'suggestedOps.irreversible.other'
  }
}

/** The pill and the unavailable button's tooltip for a group the engine would refuse outright. */
export function runBlockerKeys(blocker: RunBlocker): { pill: MessageKey; tooltip: MessageKey } {
  return RUN_BLOCKER_KEYS[blocker]
}

/** Keyed by every `RunBlocker`, so a new variant is a compile error until it has its words. */
const RUN_BLOCKER_KEYS: Record<RunBlocker, { pill: MessageKey; tooltip: MessageKey }> = {
  noTrash: { pill: 'suggestedOps.blocked.noTrash', tooltip: 'suggestedOps.blocked.noTrashTooltip' },
}

/**
 * What a row's Size and Last modified columns show.
 *
 * - `live`: Cmdr looked just now. `sizeChanged` / `modifiedChanged` say the value moved since the
 *   suggestion, so the row can show both.
 * - `missing`: the file isn't at its path any more.
 * - `snapshot`: Cmdr couldn't look (or hasn't yet), so this is what the file was when the AI
 *   suggested it, and the row labels it that way. Either value may be `null` when the
 *   suggestion recorded nothing, which the row says in words, never as a zero.
 */
export type RowFacts =
  | {
      kind: 'live'
      /** `null` for a folder, whose own stat size means nothing to a user. */
      size: number | null
      modified: number | null
      snapshotSize: number | null
      snapshotModified: number | null
      sizeChanged: boolean
      modifiedChanged: boolean
    }
  | { kind: 'missing' }
  | { kind: 'snapshot'; size: number | null; modified: number | null }

export function rowFacts(op: SuggestedOpView, live: LiveFact | undefined): RowFacts {
  if (live?.kind === 'present') {
    return {
      kind: 'live',
      size: live.size,
      modified: live.modified,
      snapshotSize: op.snapshotSize,
      snapshotModified: op.snapshotModified,
      // Nothing recorded at suggestion time means nothing to have changed FROM.
      sizeChanged: op.snapshotSize !== null && live.size !== null && op.snapshotSize !== live.size,
      modifiedChanged: op.snapshotModified !== null && live.modified !== null && op.snapshotModified !== live.modified,
    }
  }
  if (live?.kind === 'missing') return { kind: 'missing' }
  return { kind: 'snapshot', size: op.snapshotSize, modified: op.snapshotModified }
}

/** The last path segment: the name a row leads with. */
export function fileName(path: string): string {
  const trimmed = path.endsWith('/') && path.length > 1 ? path.slice(0, -1) : path
  return trimmed.slice(trimmed.lastIndexOf('/') + 1)
}

/** Everything before the name, or `''` for a bare name. */
export function parentFolder(path: string): string {
  const trimmed = path.endsWith('/') && path.length > 1 ? path.slice(0, -1) : path
  const slash = trimmed.lastIndexOf('/')
  if (slash < 0) return ''
  return slash === 0 ? '/' : trimmed.slice(0, slash)
}

/** The icon id the shared icon cache keys a file by (`ext:pdf`, or `file` with no extension).
 *  Mirrors the backend's `derive_icon_id`; an op is a file as far as the row can tell. */
export function fileIconId(path: string): string {
  const name = fileName(path)
  const dot = name.lastIndexOf('.')
  if (dot <= 0 || dot === name.length - 1) return 'file'
  return `ext:${name.slice(dot + 1).toLowerCase()}`
}
