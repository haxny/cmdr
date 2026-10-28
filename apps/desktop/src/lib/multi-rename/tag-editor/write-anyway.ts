/**
 * "Write anyway": the answer to a would-drop choice, shared by the Write dialog (`ApplyPanel`) and ⌘⏎'s choice
 * (`TagsContent`), so both write exactly the same. It waits for the calls still out, writes nothing when the view
 * changed since the choice was asked (an edit landed: the caller lists the changes again, continuing the press), and
 * otherwise stops the player (`onWriting`) and sends the would-drop files alone with frame loss allowed
 * (`writeAnywayAfter`), as part of the same press (`after`), so Undo reverses it with the first apply.
 */
import type { ContentKind } from '#lib/ipc/bindings.js'
import type { TagApplyReport, TagEditorView } from '#lib/tauri-commands/index.js'
import { writeAnywayAfter } from './apply-outcome'
import type { TagEditorState, TagStateError } from './tag-editor-state.svelte'
import { WRITE_WAIT_LIMIT_MS } from './write-wait'

/** The choice being answered: the press's applies so far, the kind they wrote as, and the view after the last. */
export interface WouldDropChoice {
  reports: TagApplyReport[]
  kind: ContentKind
  seen: TagEditorView | null
  /** The files whose kind removals the list this press wrote from showed (`removalsShown`); none for ⌘⏎. */
  allowRemovals?: readonly string[]
}

export type WriteAnywayAnswer =
  | { kind: 'written'; reports: TagApplyReport[] }
  | { kind: 'changed' }
  | { kind: 'stillBusy' }
  | { kind: 'refused'; error: TagStateError }

export async function answerWriteAnyway(
  tags: Pick<TagEditorState, 'view' | 'settled' | 'apply'>,
  choice: WouldDropChoice,
  onWriting?: () => void,
): Promise<WriteAnywayAnswer> {
  const last = choice.reports.at(-1)
  if (last === undefined) return { kind: 'changed' }
  if (!(await tags.settled(WRITE_WAIT_LIMIT_MS))) return { kind: 'stillBusy' }
  if (tags.view !== choice.seen) return { kind: 'changed' }
  onWriting?.()
  const res = await tags.apply(writeAnywayAfter(choice.kind, last, tags.view, choice.allowRemovals), {
    after: choice.reports,
  })
  return res.ok ? { kind: 'written', reports: [...choice.reports, res.value] } : { kind: 'refused', error: res.error }
}
