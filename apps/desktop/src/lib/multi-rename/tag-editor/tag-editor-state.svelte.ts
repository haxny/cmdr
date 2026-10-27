/**
 * The Tags tab's state for one Multi-Rename session: the backend's view of the tab, the last typed error, and the
 * last apply Undo can reverse. The work is the backend's (`tauri-commands/tag-editor.ts`); this holds what it answered.
 * `open()` is lazy: the sheet calls it the first time the Tags tab shows, so a sheet that never shows it asks nothing.
 * The write Undo reverses is kept in `last-tag-apply.svelte.ts`, so it outlives the sheet.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import {
  applyTags,
  closeTagEditor,
  editTags,
  getCoverImage,
  getTagChanges,
  listCoverCandidates,
  openTagEditor,
  runTagTool,
  setCoverFromFile,
  undoTagApply,
  type ApplyRequest,
  type CoverCandidate,
  type CoverImage,
  type FileChangeView,
  type RestoreReport,
  type TagApplyReport,
  type TagEdit,
  type TagEditorError,
  type TagEditorResult,
  type TagEditorView,
  type TagTool,
} from '#lib/tauri-commands/index.js'
import { getLastTagApply, setLastTagApply, type LastTagApply } from './last-tag-apply.svelte'

/** Why an Undo has nothing to reverse: no apply wrote anything yet, or the store couldn't keep the old tags. */
export type TagUndoRefusal = { kind: 'nothingToUndo' } | { kind: 'undoUnavailable' }

/** A backend error, or Undo's own refusal. */
export type TagStateError = TagEditorError | TagUndoRefusal

export type TagStateResult<T> = { ok: true; value: T } | { ok: false; error: TagStateError }

export interface ApplyOptions {
  /** This apply finishes the last one ("Write anyway"): Undo reverses both, not just this one. */
  continuing?: boolean
}

export interface TagEditorState {
  readonly view: TagEditorView | null
  readonly error: TagStateError | null
  readonly busy: boolean
  /** The last write Undo can reverse, or `null` when there's none. */
  readonly undoable: LastTagApply | null
  open(): Promise<void>
  /** Reads every file again, dropping every unwritten edit (after "changed on disk since it was read"). */
  reload(): Promise<void>
  edit(edits: TagEdit[]): Promise<void>
  tool(tool: TagTool): Promise<void>
  /** Resolves when every call now out has answered (an edit a blur just started included). */
  settled(): Promise<void>
  /** The cover every file shares, with its bytes; `null` when there is none, the files differ, or it can't be read. */
  coverImage(): Promise<CoverImage | null>
  /** The images in the session's folder a cover can come from. */
  coverCandidates(): Promise<TagStateResult<CoverCandidate[]>>
  /** Gives every file the folder image `fileName` as its cover. */
  coverFromFile(fileName: string): Promise<void>
  changes(): Promise<TagStateResult<FileChangeView[]>>
  apply(request: ApplyRequest, options?: ApplyOptions): Promise<TagStateResult<TagApplyReport>>
  /** Writes the old tags of the last write back, the last apply first. A failure leaves the rest undoable. */
  undo(): Promise<TagStateResult<RestoreReport>>
  close(): Promise<void>
}

const log = getAppLogger('multiRename')

export function createTagEditorState(sessionId: string): TagEditorState {
  let view = $state.raw<TagEditorView | null>(null)
  let error = $state.raw<TagStateError | null>(null)
  let pending = $state(0)
  const inFlight = new Set<Promise<unknown>>()
  let opening: Promise<void> | null = null
  let started = false
  /** Set by `close()`: a reply that lands after it is dropped. */
  let closed = false
  /** Why Undo has nothing to reverse, when the store holds no write. */
  let refusal: TagUndoRefusal = { kind: 'nothingToUndo' }

  /** Runs one backend call with `busy` on, keeps its error, and hands the result back. */
  async function track<T>(call: () => Promise<TagEditorResult<T>>): Promise<TagEditorResult<T>> {
    pending += 1
    const running = call()
    inFlight.add(running)
    try {
      const res = await running
      error = res.ok ? null : res.error
      return res
    } finally {
      inFlight.delete(running)
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
    get undoable() {
      return getLastTagApply()
    },
    open,
    edit: (edits) => replaceView(() => editTags(sessionId, edits)),
    tool: (tool) => replaceView(() => runTagTool(sessionId, tool)),
    settled: async () => {
      while (inFlight.size > 0) await Promise.allSettled([...inFlight])
    },
    async coverImage() {
      const res = await getCoverImage(sessionId)
      if (res.ok) return res.value
      // Only the thumbnail is missing; the kind is no path or value.
      log.warn("couldn't read the cover image: {kind}", { kind: res.error.kind })
      return null
    },
    coverCandidates: () => track(() => listCoverCandidates(sessionId)),
    coverFromFile: (fileName) => replaceView(() => setCoverFromFile(sessionId, fileName)),
    changes: () => track(() => getTagChanges(sessionId)),
    async apply(request, options = {}) {
      const res = await track(() => applyTags(sessionId, request))
      if (!res.ok) return res
      refusal = res.value.undoUnavailable ? { kind: 'undoUnavailable' } : { kind: 'nothingToUndo' }
      setLastTagApply(nextUndoable(options.continuing === true ? getLastTagApply() : null, res.value))
      await replaceView(() => openTagEditor(sessionId))
      return res
    },
    async undo() {
      const last = getLastTagApply()
      if (last === null) return { ok: false, error: refusal }
      // Taken first, so a second press doesn't undo the same write twice.
      setLastTagApply(null)
      const res = await undoAll(last, (applyId) => track(() => undoTagApply(applyId)))
      // Even a failed undo may have restored some files: the view reads them again, and the error stays.
      if (started) await replaceView(() => openTagEditor(sessionId))
      if (!res.ok) error = res.error
      return res
    },
    reload: () => replaceView(() => openTagEditor(sessionId, true)),
    async close() {
      if (!started) return
      started = false
      closed = true
      refusal = { kind: 'nothingToUndo' }
      view = null
      try {
        await closeTagEditor(sessionId)
      } catch (e) {
        // Nothing to show: the sheet is gone. No session id, path or tag value goes in the log.
        log.warn("couldn't end the tag session: {reason}", { reason: e })
      }
    },
  }
}

/** The write Undo reverses after `report`: this apply's id (when it wrote and kept the old tags) after `before`'s. */
function nextUndoable(before: LastTagApply | null, report: TagApplyReport): LastTagApply | null {
  const added = report.applyId !== null && !report.undoUnavailable && report.written.length > 0 ? [report.applyId] : []
  const applyIds = [...(before?.applyIds ?? []), ...added]
  if (applyIds.length === 0) return null
  return { applyIds, written: (before?.written ?? 0) + report.written.length }
}

/**
 * Undoes `last`'s applies, the last one first. An apply the backend couldn't undo (an error, or files it couldn't
 * restore, whose record it keeps) stays in the store, so the user can try again.
 */
async function undoAll(
  last: LastTagApply,
  undoOne: (applyId: string) => Promise<TagEditorResult<RestoreReport>>,
): Promise<TagStateResult<RestoreReport>> {
  const merged: RestoreReport = { restored: [], missing: [], failed: [] }
  const kept: string[] = []
  const left = [...last.applyIds]
  for (let applyId = left.pop(); applyId !== undefined; applyId = left.pop()) {
    const res = await undoOne(applyId)
    if (!res.ok) {
      setLastTagApply({ ...last, applyIds: [...left, applyId, ...kept] })
      return res
    }
    merged.restored.push(...res.value.restored)
    merged.missing.push(...res.value.missing)
    merged.failed.push(...res.value.failed)
    if (res.value.failed.length > 0) kept.unshift(applyId)
  }
  setLastTagApply(kept.length > 0 ? { ...last, applyIds: kept } : null)
  return { ok: true, value: merged }
}
