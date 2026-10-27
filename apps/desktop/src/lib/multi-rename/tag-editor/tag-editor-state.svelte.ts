/**
 * The Tags tab's state for one Multi-Rename session: the backend's view of the tab, the last typed error, and the
 * last apply Undo can reverse. The work is the backend's (`tauri-commands/tag-editor.ts`); this holds what it answered.
 * `open()` is lazy: the sheet calls it the first time the Tags tab shows, so a sheet that never shows it asks nothing.
 */

import {
  applyTags,
  closeTagEditor,
  editTags,
  getTagChanges,
  openTagEditor,
  runTagTool,
  undoTagApply,
  type ApplyRequest,
  type FileChangeView,
  type RestoreReport,
  type TagApplyReport,
  type TagEdit,
  type TagEditorError,
  type TagEditorResult,
  type TagEditorView,
  type TagTool,
} from '#lib/tauri-commands/index.js'

/** Why an Undo has nothing to reverse: no apply wrote anything yet, or the store couldn't keep the old tags. */
export type TagUndoRefusal = { kind: 'nothingToUndo' } | { kind: 'undoUnavailable' }

/** A backend error, or Undo's own refusal. */
export type TagStateError = TagEditorError | TagUndoRefusal

export type TagStateResult<T> = { ok: true; value: T } | { ok: false; error: TagStateError }

export interface TagEditorState {
  readonly view: TagEditorView | null
  readonly error: TagStateError | null
  readonly busy: boolean
  open(): Promise<void>
  edit(edits: TagEdit[]): Promise<void>
  tool(tool: TagTool): Promise<void>
  changes(): Promise<TagStateResult<FileChangeView[]>>
  apply(request: ApplyRequest): Promise<TagStateResult<TagApplyReport>>
  undo(): Promise<TagStateResult<RestoreReport>>
  close(): Promise<void>
}

export function createTagEditorState(sessionId: string): TagEditorState {
  let view = $state.raw<TagEditorView | null>(null)
  let error = $state.raw<TagStateError | null>(null)
  let pending = $state(0)
  let opening: Promise<void> | null = null
  let started = false
  /** Set by `close()`: a reply that lands after it is dropped. */
  let closed = false
  /** The last apply's id while it can be undone; otherwise why not. */
  let undoable: string | TagUndoRefusal = { kind: 'nothingToUndo' }

  /** Runs one backend call with `busy` on, keeps its error, and hands the result back. */
  async function track<T>(call: () => Promise<TagEditorResult<T>>): Promise<TagEditorResult<T>> {
    pending += 1
    try {
      const res = await call()
      error = res.ok ? null : res.error
      return res
    } finally {
      pending -= 1
    }
  }

  async function replaceView(call: () => Promise<TagEditorResult<TagEditorView>>): Promise<void> {
    const res = await track(call)
    if (res.ok && !closed) view = res.value
  }

  async function open(): Promise<void> {
    if (view !== null) return
    opening ??= replaceView(() => openTagEditor(sessionId)).finally(() => {
      opening = null
    })
    started = true
    await opening
  }

  return {
    get view() {
      return view
    },
    get error() {
      return error
    },
    get busy() {
      return pending > 0
    },
    open,
    edit: (edits) => replaceView(() => editTags(sessionId, edits)),
    tool: (tool) => replaceView(() => runTagTool(sessionId, tool)),
    changes: () => track(() => getTagChanges(sessionId)),
    async apply(request) {
      const res = await track(() => applyTags(sessionId, request))
      if (!res.ok) return res
      if (res.value.undoUnavailable) undoable = { kind: 'undoUnavailable' }
      else undoable = res.value.applyId ?? { kind: 'nothingToUndo' }
      await replaceView(() => openTagEditor(sessionId))
      return res
    },
    async undo() {
      if (typeof undoable !== 'string') return { ok: false, error: undoable }
      const applyId = undoable
      undoable = { kind: 'nothingToUndo' }
      const res = await track(() => undoTagApply(applyId))
      // A failed undo leaves the apply undoable, so the user can retry.
      if (!res.ok) undoable = applyId
      else await replaceView(() => openTagEditor(sessionId))
      return res
    },
    async close() {
      if (!started) return
      started = false
      closed = true
      undoable = { kind: 'nothingToUndo' }
      view = null
      await closeTagEditor(sessionId)
    },
  }
}
