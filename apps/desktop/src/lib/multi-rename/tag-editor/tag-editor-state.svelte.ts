/**
 * The Tags tab's state for one Multi-Rename session: the backend's view of the tab, the last typed error, and the
 * last apply Undo can reverse. The work is the backend's (`tauri-commands/tag-editor.ts`); this holds what it answered.
 * `open()` is lazy: the sheet calls it each time the Tags tab shows, so a sheet that never shows it asks nothing; once a
 * view is there it asks again for the session's current order (a reorder on the Rename tab), keeping the unwritten
 * edits, which live in the backend session.
 * The write Undo reverses is kept in `last-tag-apply.svelte.ts`, so it outlives the sheet.
 */

import type { ContentKind, TagTextTool } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  applyTags,
  closeTagEditor,
  editTags,
  getCoverImage,
  getTagChanges,
  listCoverCandidates,
  openTagEditor,
  previewTagsFromNames,
  runTagTextTool,
  runTagTool,
  setCoverFromFile,
  undoTagApply,
  type ApplyRequest,
  type CoverCandidate,
  type FileChangeView,
  type NamePattern,
  type NamePreviewRow,
  type RestoreReport,
  type TagApplyReport,
  type TagEdit,
  type TagEditorError,
  type TagEditorResult,
  type TagEditorView,
  type TagTool,
} from '#lib/tauri-commands/index.js'
import { getLastTagApply, setLastTagApply, type LastTagApply, type UndoableApply } from './last-tag-apply.svelte'

/** Why an Undo has nothing to reverse: no apply wrote anything yet, or the store couldn't keep the old tags. */
export type TagUndoRefusal = { kind: 'nothingToUndo' } | { kind: 'undoUnavailable' }

/** A backend error, or Undo's own refusal. */
export type TagStateError = TagEditorError | TagUndoRefusal

export type TagStateResult<T> = { ok: true; value: T } | { ok: false; error: TagStateError }

/** An undo's answer: on a failure, what it restored before it stopped comes with the error. */
export type TagUndoResult =
  | { ok: true; value: RestoreReport }
  | { ok: false; error: TagStateError; restored: RestoreReport }

export interface ApplyOptions {
  /**
   * The earlier applies of the same "Write tags" press (before a "Write anyway"). When one of them wrote, Undo
   * reverses this apply with them; when none did, this apply starts the write Undo reverses on its own.
   */
  after?: readonly TagApplyReport[]
}

export interface TagEditorState {
  /** The backend tag session this state talks to (the player mints its audio URLs in it). */
  readonly sessionId: string
  readonly view: TagEditorView | null
  readonly error: TagStateError | null
  readonly busy: boolean
  /** The last write Undo can reverse, or `null` when there's none. */
  readonly undoable: LastTagApply | null
  /** Fetches the view; when one is there already, fetches it again in the session's current order. */
  open(): Promise<void>
  /** Reads every file again, dropping every unwritten edit (after "changed on disk since it was read"). */
  reload(): Promise<void>
  edit(edits: TagEdit[]): Promise<void>
  /** Runs `tool` over the tracks at `scope` (their paths), or over every track when it's absent or `null`. */
  tool(tool: TagTool, scope?: string[] | null): Promise<void>
  /**
   * Runs find/replace or a field-from-mask format over the tracks at `scope` (or every track), replacing the view like
   * `tool`. `format` turns the field's FIRST value into the one rendered value.
   */
  textTool(tool: TagTextTool, scope?: string[] | null): Promise<void>
  /** What `pattern` would read from the file names of `scope` (or of every track), changing nothing. */
  previewFromNames(pattern: NamePattern, scope?: string[] | null): Promise<TagStateResult<NamePreviewRow[]>>
  /**
   * Resolves `true` when every call now out has answered (an edit a blur just started included), or `false` after
   * `limitMs` (default `SETTLE_LIMIT_MS`) with one still out, say on a stalled network mount, so a caller can go on.
   */
  settled(limitMs?: number): Promise<boolean>
  /** The cover every file shares, as a `data:` URL; `null` when there is none, the files differ, or it can't be read. */
  coverImage(): Promise<string | null>
  /** The images in the session's folder a cover can come from. */
  coverCandidates(): Promise<TagStateResult<CoverCandidate[]>>
  /** Gives every file the folder image `fileName` as its cover. */
  coverFromFile(fileName: string): Promise<void>
  /** What a write as `kind` would change, with what the kind removes beyond the fields. */
  changes(kind: ContentKind): Promise<TagStateResult<FileChangeView[]>>
  apply(request: ApplyRequest, options?: ApplyOptions): Promise<TagStateResult<TagApplyReport>>
  /** Writes the old tags of the last write back, the last apply first. A failure leaves the rest undoable. */
  undo(): Promise<TagUndoResult>
  close(): Promise<void>
}

const log = getAppLogger('multiRename')

/** How long `settled()` waits for the calls out before it gives up, so a hung call can't hold the sheet open. */
export const SETTLE_LIMIT_MS = 5_000

export function createTagEditorState(sessionId: string): TagEditorState {
  let view = $state.raw<TagEditorView | null>(null)
  let error = $state.raw<TagStateError | null>(null)
  let pending = $state(0)
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping of pending promises, nothing renders from it
  const inFlight = new Set<Promise<unknown>>()
  let opening: Promise<void> | null = null
  let started = false
  /** Set by `close()`: a reply that lands after it is dropped. */
  let closed = false
  /** Why Undo has nothing to reverse, when the store holds no write. */
  let refusal: TagUndoRefusal = { kind: 'nothingToUndo' }
  /** Counts view requests; an answer to an older one than the last that landed is dropped. */
  let viewAsked = 0
  let viewShown = 0

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
    const asked = ++viewAsked
    const res = await track(call)
    if (!res.ok || closed || asked < viewShown) return
    viewShown = asked
    view = res.value
  }

  async function settled(limitMs = SETTLE_LIMIT_MS): Promise<boolean> {
    let timer: ReturnType<typeof setTimeout> | undefined
    const gaveUp = new Promise<false>((resolve) => {
      timer = setTimeout(() => {
        resolve(false)
      }, limitMs)
    })
    const drained = (async () => {
      while (inFlight.size > 0) await Promise.allSettled([...inFlight])
      return true as const
    })()
    try {
      return await Promise.race([drained, gaveUp])
    } finally {
      clearTimeout(timer)
    }
  }

  async function open(): Promise<void> {
    if (view !== null) {
      // The Rename tab may have reordered the files: the session answers in its current order, edits kept.
      await settled()
      await replaceView(() => openTagEditor(sessionId))
      return
    }
    opening ??= replaceView(() => openTagEditor(sessionId)).finally(() => {
      opening = null
    })
    started = true
    await opening
  }

  return {
    sessionId,
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
      return undoableWrite()
    },
    open,
    edit: (edits) => replaceView(() => editTags(sessionId, edits)),
    tool: (tool, scope = null) => replaceView(() => runTagTool(sessionId, tool, scope)),
    textTool: (tool, scope = null) => replaceView(() => runTagTextTool(sessionId, tool, scope)),
    previewFromNames: (pattern, scope = null) => track(() => previewTagsFromNames(sessionId, pattern, scope)),
    settled,
    async coverImage() {
      const res = await getCoverImage(sessionId)
      if (res.ok) return res.value
      // Only the thumbnail is missing; the kind is no path or value.
      log.warn("couldn't read the cover image: {kind}", { kind: res.error.kind })
      return null
    },
    coverCandidates: () => track(() => listCoverCandidates(sessionId)),
    coverFromFile: (fileName) => replaceView(() => setCoverFromFile(sessionId, fileName)),
    changes: (kind) => track(() => getTagChanges(sessionId, kind)),
    async apply(request, options = {}) {
      const res = await track(() => applyTags(sessionId, request))
      if (!res.ok) return res
      const continuing = (options.after ?? []).some((earlier) => earlier.written.length > 0)
      // A write that wrote nothing leaves the last one undoable: there's nothing newer to protect.
      if (continuing || res.value.written.length > 0) {
        refusal = res.value.undoUnavailable ? { kind: 'undoUnavailable' } : { kind: 'nothingToUndo' }
        setLastTagApply(nextUndoable(continuing ? getLastTagApply() : null, res.value))
      }
      await replaceView(() => openTagEditor(sessionId))
      return res
    },
    async undo() {
      const last = undoableWrite()
      if (last === null) return { ok: false, error: refusal, restored: emptyRestore() }
      // Taken first, so a second press doesn't undo the same write twice.
      setLastTagApply(null)
      const res = await undoAll(last, (applyId) => track(() => undoTagApply(applyId, sessionId)))
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

function emptyRestore(): RestoreReport {
  return { restored: [], missing: [], changed: [], failed: [] }
}

/** The write Undo reverses after `report`: this apply (when it wrote and kept the old tags) after `before`'s. */
function nextUndoable(before: LastTagApply | null, report: TagApplyReport): LastTagApply | null {
  const keptOld = report.applyId !== null && !report.undoUnavailable && report.written.length > 0
  const added: UndoableApply[] =
    keptOld && report.applyId !== null ? [{ applyId: report.applyId, written: report.written.length }] : []
  const applies = [...(before?.applies ?? []), ...added]
  const partial = (before?.partial ?? false) || (report.undoUnavailable && report.written.length > 0)
  // Kept with no applies when the write can't be undone at all, so a "Write anyway" after it is marked partial.
  return applies.length === 0 && !partial ? null : { applies, partial }
}

/** The stored write while Undo has something to reverse. */
function undoableWrite(): LastTagApply | null {
  const last = getLastTagApply()
  return last !== null && last.applies.length > 0 ? last : null
}

/**
 * Undoes `last`'s applies, the last one first. An apply the backend couldn't undo (an error, or files it couldn't
 * restore, whose record it keeps) stays in the store with its own count, so a retry says how many files are left.
 */
async function undoAll(
  last: LastTagApply,
  undoOne: (applyId: string) => Promise<TagEditorResult<RestoreReport>>,
): Promise<TagUndoResult> {
  const merged = emptyRestore()
  const kept: UndoableApply[] = []
  const left = [...last.applies]
  for (let apply = left.pop(); apply !== undefined; apply = left.pop()) {
    const res = await undoOne(apply.applyId)
    if (!res.ok) {
      setLastTagApply({ ...last, applies: [...left, apply, ...kept] })
      return { ok: false, error: res.error, restored: merged }
    }
    merged.restored.push(...res.value.restored)
    merged.missing.push(...res.value.missing)
    merged.changed.push(...res.value.changed)
    merged.failed.push(...res.value.failed)
    if (res.value.failed.length > 0) kept.unshift({ ...apply, written: res.value.failed.length })
  }
  setLastTagApply(kept.length > 0 ? { ...last, applies: kept } : null)
  return { ok: true, value: merged }
}
