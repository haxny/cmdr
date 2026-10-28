/**
 * The Tags window's state for one Multi-Rename session: the backend's view of the tracks, the last typed error, and the
 * last write Undo can reverse. The work is the backend's (`tauri-commands/tag-editor.ts`); this holds what it answered.
 * `open()` is lazy: the window calls it when it mounts; once a view is there it asks again for the session's
 * current order (the session's names can be reordered), keeping the unwritten edits, which live in the backend
 * session.
 * The write Undo reverses is the backend's, kept per folder for the app's lifetime (`last-tag-apply.ts`), so it
 * outlives the window: this only mirrors it, asking again on open and after every write and undo.
 */

import type { ContentKind, TagTextTool } from '#lib/ipc/bindings.js'
import { getAppLogger } from '#lib/logging/logger.js'
import {
  applyTags,
  closeTagEditor,
  editTags,
  getCoverImage,
  getCoverThumbnail,
  getTagChanges,
  lastTagApply,
  listCoverCandidates,
  listTagCovers,
  openTagEditor,
  previewTagsFromNames,
  runTagTextTool,
  runTagTool,
  setCoverFromBytes,
  setCoverFromFile,
  setCoverFromPath,
  sortTagTracks,
  undoTagApply,
  undoTagEdit,
  redoTagEdit,
  useCoverForAll,
  type ApplyRequest,
  type CoverCandidate,
  type CoverChoice,
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
  type TrackSort,
} from '#lib/tauri-commands/index.js'
import type { LastTagApply } from './last-tag-apply'

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
  /** The last failed edit, tool, cover pick or open; a failed change list or write is the Apply panel's to word. */
  readonly error: TagStateError | null
  readonly busy: boolean
  /** A write or an undo is on its way: the window can't close under it. */
  readonly writing: boolean
  /** An edit, tool, or cover change is on its way (a read isn't): closing asks about it. */
  readonly editing: boolean
  /** The last write Undo can reverse, or `null` when there's none. */
  readonly undoable: LastTagApply | null
  /** Fetches the view; when one is there already, fetches it again in the session's current order. */
  open(): Promise<void>
  /** Reads every file again, dropping every unwritten edit (after "changed on disk since it was read"). */
  reload(): Promise<void>
  edit(edits: TagEdit[]): Promise<void>
  /**
   * Reverses the last pending edit (one edit call: a batch, a tool run, a cover change) and takes the view it answers
   * with. Waits for the edits still out first. Not the write undo (`undo`); `view.canUndoEdit` says whether there is one.
   * Resolves whether the view changed.
   */
  undoEdit(): Promise<boolean>
  /** Re-applies the last undone edit (`view.canRedoEdit`). */
  redoEdit(): Promise<boolean>
  /**
   * Puts the tracks in `sort` order and takes the view it answers with (`view.sort` is the order now). The backend
   * reorders only the session's names: no edit and no file changes, and the next `[C]` or Renumber counts in this order.
   */
  sortTracks(sort: TrackSort): Promise<void>
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
  /**
   * A cover as a `data:` URL: the one `key` names (a `CoverChoice.key`), or without one the cover every file shares;
   * `null` when there is none, the files differ, or it can't be read.
   */
  coverImage(key?: string | null): Promise<string | null>
  /** A tile's small picture of the cover `key` names; `null` when there is none or it can't be had (logged by kind). */
  coverThumbnail(key: string): Promise<string | null>
  /** Every distinct cover with how many files carry it (the files without one last), or `null` when that failed. */
  coverChoices(): Promise<CoverChoice[] | null>
  /** Gives every file the cover `key` names, in memory: a change Write lists. */
  useCoverForAll(key: string): Promise<void>
  /** Gives every file a pasted picture (its bytes) as its cover, in memory. */
  coverFromBytes(data: Uint8Array): Promise<void>
  /** Gives every file a dropped picture file as its cover, in memory. */
  coverFromPath(path: string): Promise<void>
  /** The images in the session's folder a cover can come from. */
  coverCandidates(): Promise<TagStateResult<CoverCandidate[]>>
  /** Gives every file the folder image `fileName` as its cover. */
  coverFromFile(fileName: string): Promise<void>
  /** What a write as `kind` would change, with what the kind removes beyond the fields. A failure leaves `error` alone. */
  changes(kind: ContentKind): Promise<TagStateResult<FileChangeView[]>>
  /** Writes the edits; a refusal comes back typed and leaves `error` alone. */
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
  let writes = $state(0)
  let edits = $state(0)
  /** The backend's last write of this folder, mirrored. */
  let last = $state.raw<LastTagApply | null>(null)
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping of pending promises, nothing renders from it
  const inFlight = new Set<Promise<unknown>>()
  let opening: Promise<void> | null = null
  let started = false
  /** Set by `close()`: a reply that lands after it is dropped. */
  let closed = false
  /** Counts view requests; an answer to an older one than the last that landed is dropped. */
  let viewAsked = 0
  let viewShown = 0

  /**
   * Runs one backend call with `busy` on, keeps its error, and hands the result back. `clears` says whether a success
   * clears `error` (a call that owns only some errors passes which); `null` leaves `error` alone either way, for a call
   * whose caller words its own failure (the Apply panel's change list and write).
   */
  async function track<T>(
    call: () => Promise<TagEditorResult<T>>,
    clears: ((current: TagStateError) => boolean) | null = () => true,
  ): Promise<TagEditorResult<T>> {
    pending += 1
    const running = call()
    inFlight.add(running)
    try {
      const res = await running
      if (clears === null) return res
      if (!res.ok) error = res.error
      else if (error !== null && clears(error)) error = null
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

  /** Runs an edit-like call (it changes the edits): `editing` is on meanwhile. */
  async function editView(call: () => Promise<TagEditorResult<TagEditorView>>): Promise<void> {
    edits += 1
    try {
      await replaceView(call)
    } finally {
      edits -= 1
    }
  }

  /** Runs a write or an undo: `writing` is on meanwhile. */
  async function whileWriting<T>(run: () => Promise<T>): Promise<T> {
    writes += 1
    try {
      return await run()
    } finally {
      writes -= 1
    }
  }

  /** Mirrors the backend's last write of this folder. A failed ask keeps what was there (logged by kind). */
  async function refreshLast(): Promise<void> {
    try {
      const res = await lastTagApply(sessionId)
      if (closed) return
      if (res.ok) last = res.value
      else log.warn("couldn't ask for the last tag write: {kind}", { kind: res.error.kind })
    } catch (e) {
      log.warn("couldn't ask for the last tag write: {reason}", { reason: e })
    }
  }

  /** The mirrored last write while Undo has something to reverse. */
  function undoableWrite(): LastTagApply | null {
    return last !== null && last.applies.length > 0 ? last : null
  }

  /** Why Undo has nothing to reverse: a last write none of whose files kept their old tags, or none at all. */
  function refusal(): TagUndoRefusal {
    return last !== null && last.applies.length === 0 && last.partial
      ? { kind: 'undoUnavailable' }
      : { kind: 'nothingToUndo' }
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

  /**
   * Runs an undo or redo of an edit once the edits still out have landed. `false` when it didn't run (an edit stayed
   * out past the limit: a late one would land after it) or the call failed.
   */
  async function stepEdit(call: () => Promise<TagEditorResult<TagEditorView>>): Promise<boolean> {
    if (!(await settled())) return false
    const before = view
    await editView(call)
    return view !== before && error === null
  }

  async function open(): Promise<void> {
    if (view !== null) {
      // The session's order may have changed the files: the session answers in its current order, edits kept.
      await settled()
      await replaceView(() => openTagEditor(sessionId))
      return
    }
    opening ??= Promise.all([replaceView(() => openTagEditor(sessionId)), refreshLast()])
      .then(() => undefined)
      .finally(() => {
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
    get writing() {
      return writes > 0
    },
    get editing() {
      return edits > 0
    },
    get undoable() {
      return undoableWrite()
    },
    open,
    edit: (batch) => editView(() => editTags(sessionId, batch)),
    async undoEdit() {
      return await stepEdit(() => undoTagEdit(sessionId))
    },
    async redoEdit() {
      return await stepEdit(() => redoTagEdit(sessionId))
    },
    // Not an edit: it changes the order only, so `editing` stays off.
    sortTracks: (sort) => replaceView(() => sortTagTracks(sessionId, sort)),
    tool: (tool, scope = null) => editView(() => runTagTool(sessionId, tool, scope)),
    textTool: (tool, scope = null) => editView(() => runTagTextTool(sessionId, tool, scope)),
    // A preview owns only a bad pattern: a good one leaves any other notice (a failed edit, a cover) standing.
    previewFromNames: (pattern, scope = null) =>
      track(
        () => previewTagsFromNames(sessionId, pattern, scope),
        (current) => current.kind === 'badPattern',
      ),
    settled,
    async coverImage(key = null) {
      const res = await getCoverImage(sessionId, key)
      if (res.ok) return res.value
      // Only the thumbnail is missing; the kind is no path or value.
      log.warn("couldn't read the cover image: {kind}", { kind: res.error.kind })
      return null
    },
    async coverThumbnail(key) {
      const res = await getCoverThumbnail(sessionId, key)
      if (res.ok) return res.value
      // Only the tile's picture is missing; the kind is no path or value.
      log.warn("couldn't make a cover thumbnail: {kind}", { kind: res.error.kind })
      return null
    },
    async coverChoices() {
      try {
        const res = await listTagCovers(sessionId)
        if (res.ok) return res.value
        // The section words it; the kind is no path or value.
        log.warn("couldn't list the covers: {kind}", { kind: res.error.kind })
      } catch (e) {
        log.warn("couldn't list the covers: {reason}", { reason: e })
      }
      return null
    },
    useCoverForAll: (key) => editView(() => useCoverForAll(sessionId, key)),
    coverFromBytes: (data) => editView(() => setCoverFromBytes(sessionId, data)),
    coverFromPath: (path) => editView(() => setCoverFromPath(sessionId, path)),
    coverCandidates: () => track(() => listCoverCandidates(sessionId)),
    coverFromFile: (fileName) => editView(() => setCoverFromFile(sessionId, fileName)),
    // The Apply panel words a failed list or write next to its buttons; the window's error is for edits.
    changes: (kind) => track(() => getTagChanges(sessionId, kind), null),
    async apply(request, options = {}) {
      // A "Write anyway" after a press that wrote continues that write, so Undo reverses both.
      const continues = (options.after ?? []).some((earlier) => earlier.written.length > 0)
      const res = await whileWriting(async () => {
        const answer = await track(() => applyTags(sessionId, request, continues), null)
        if (answer.ok) await refreshLast()
        return answer
      })
      if (res.ok) await replaceView(() => openTagEditor(sessionId))
      return res
    },
    async undo() {
      const undoing = undoableWrite()
      if (undoing === null) return { ok: false, error: refusal(), restored: emptyRestore() }
      // Taken first, so a second press doesn't undo the same write twice; the backend's answer comes back after.
      last = null
      const res = await whileWriting(async () => {
        const undone = await undoAll(undoing, (applyId) => track(() => undoTagApply(applyId, sessionId)))
        await refreshLast()
        return undone
      })
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
      last = null
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

/**
 * Undoes `last`'s applies, the last one first, and stops at the first that fails. The backend keeps the folder's last
 * write in step (an apply put back goes, one with files left stays with their count), so the caller asks it again.
 */
async function undoAll(
  last: LastTagApply,
  undoOne: (applyId: string) => Promise<TagEditorResult<RestoreReport>>,
): Promise<TagUndoResult> {
  const merged = emptyRestore()
  for (const apply of [...last.applies].reverse()) {
    const res = await undoOne(apply.applyId)
    if (!res.ok) return { ok: false, error: res.error, restored: merged }
    merged.restored.push(...res.value.restored)
    merged.missing.push(...res.value.missing)
    merged.changed.push(...res.value.changed)
    merged.failed.push(...res.value.failed)
  }
  return { ok: true, value: merged }
}
