/**
 * The Multi-Rename sheet's state: the spec the fields edit, the live preview, the
 * window of rows the list draws, the presets (and which one the fields came from),
 * and Start. One instance per open sheet, over one backend session. It opens on the
 * settings (and the preset) the last sheet closed with (`persist`), as TC does; an
 * edit made before they arrive wins.
 *
 * The files and their names live in the backend session (`src-tauri/src/multi_rename/session.rs`):
 * a preview answers its id, the counts, and the first rows, and the list pages in
 * the rows it scrolls to (`show`, which `source` hands `ColumnList`), from every row
 * or from the problem rows alone. Start sends the session and preview ids, never names.
 * Results (⌥⏎) is the backend's too: it writes the shown preview to a file, reads the
 * user's edits back into the session, and the next preview shows them.
 *
 * The preview reruns 120 ms after the last edit, and a generation counter drops
 * an answer a newer edit overtook, so fast typing never shows an older preview.
 *
 * The list has one cursor (`cursorIndex`, `cursorRow`), on the top row at first. It stays
 * on its row (`PreviewRow.row`) across previews: with every row listed a row's index IS
 * its `row`; among the problems it's found again in the first page that lands. ⇧↑ / ⇧↓
 * move the cursor row in the session's rename order (`moveCursorRow`), one at a time.
 *
 * The session reads its files' EXIF and tags in the background from the moment it opens.
 * The state follows that read (`media`), asks for the rows in view first, and, while a mask
 * uses a media field, previews again as reads land. A Rename pressed before the read is done
 * waits for it (`queued`) and then runs on the final preview; a second press or any edit
 * cancels it.
 */

import {
  applyMultiRename,
  clearMultiRenameNames,
  deleteMultiRenamePreset,
  getMultiRenameFieldSamples,
  getMultiRenameLastSettings,
  getMultiRenameMediaOverview,
  getMultiRenameMediaProgress,
  getMultiRenamePresets,
  getMultiRenamePreviewRows,
  getMultiRenameRangeSamples,
  moveMultiRenamePreset,
  moveMultiRenameRow,
  onMultiRenameMediaProgress,
  previewMultiRename,
  prioritizeMultiRenameMedia,
  readMultiRenameNames,
  renameMultiRenamePreset,
  saveMultiRenameLastSettings,
  saveMultiRenamePreset,
  updateMultiRenamePreset,
  writeMultiRenameNames,
  type MediaProgress,
  type MultiRenameError,
  type MultiRenameMediaProgress,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewCounts,
  type PreviewFilter,
  type PreviewRow,
  type RowMove,
} from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { SvelteMap } from 'svelte/reactivity'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { DEFAULT_SPEC, PRESET_SEEDS, specsEqual } from './spec'
import { usesMediaFields } from './media-token'
import { maskUsesMedia, patchSpec } from './mask-syntax'
import {
  rowUnderBrokenMask,
  rowsAround,
  type MultiRenameState,
  type RowWindow,
  type SampleRows,
} from './multi-rename-state-api'

export const PREVIEW_DELAY_MS = 120

/** While reads land, the preview reruns at most this often (the backend reports every 250 ms). */
export const MEDIA_REFRESH_MS = 500

/** The most rows one page request asks for (the backend's `MAX_PAGE`). */
const PAGE_LIMIT = 1000

/** Rows held past this many are dropped when they're far from the window. */
const HELD_ROWS = 3000

const NO_COUNTS: PreviewCounts = { ready: 0, unchanged: 0, problems: 0, reading: 0 }

/** How the backend tells two preset names apart (`presets.rs`'s `name_key`). */
const nameKey = (name: string): string => name.trim().toLowerCase()

export { rowsAround, type MultiRenameState, type RowWindow, type SampleRows } from './multi-rename-state-api'

export function createMultiRenameState(sessionId: string): MultiRenameState {
  let spec = $state<MultiRenameSpec>({ ...DEFAULT_SPEC })
  let previewId = $state<number | null>(null)
  let counts = $state.raw<PreviewCounts>(NO_COUNTS)
  let problemsOnly = $state(false)
  // Keyed by place in the list (every row, or the problem rows alone). Replaced with
  // each preview and each switch of the list; pages land in it as they arrive.
  let rows = $state.raw(new SvelteMap<number, PreviewRow>())
  let error = $state<MultiRenameError | null>(null)
  let syntaxSwitched = $state(false)
  let presets = $state.raw<MultiRenamePreset[]>([])
  let loadedId = $state<string | null>(null)
  let applying = $state(false)
  let applyError = $state<MultiRenameError | null>(null)
  let editedCount = $state(0)
  let waiting = $state(0)
  // Where the cursor was put; `cursorIndex` clamps it to what's listed.
  let cursor = $state(0)
  // Among the problems a new preview may move the cursor's row: its `row`, to find again
  // in the first page that lands.
  let cursorAnchor: number | null = null
  // Moves run one after another, so each starts from where the last left the row.
  let moving: Promise<unknown> = Promise.resolve()
  let generation = 0
  let timer: ReturnType<typeof setTimeout> | null = null
  let shown: RowWindow = { start: 0, end: 0 }
  let fetching: string | null = null
  // An edit made before the last settings arrive wins over them.
  let touched = false
  let media = $state.raw<MediaProgress | null>(null)
  let mediaTimer: ReturnType<typeof setTimeout> | null = null
  let unlistenMedia: UnlistenFn | null = null
  let disposed = false
  // Read through a call after an await: TypeScript keeps a plain `disposed` read narrowed to its pre-await value.
  const isDisposed = (): boolean => disposed
  // The queued Rename's resolve; `queued` mirrors it for the button.
  let queue: ((started: MultiRenameStarted | null) => void) | null = null
  let queued = $state(false)
  // The queued Rename is on its way to the backend: too late to cancel.
  let launching = $state(false)
  // An edit came while launching: a `stillReading` answer ends the queue rather than retrying.
  let cancelRequested = false
  // Bumped by every edit and cancel, so a press can tell one came while it waited on the backend.
  let edits = 0
  let prioritized = ''

  const totalOf = (c: PreviewCounts): number => c.ready + c.unchanged + c.problems + c.reading
  const usesMedia = (): boolean =>
    maskUsesMedia(spec.nameMask, spec.maskSyntax ?? 'cmdr') || usesMediaFields(spec.extensionMask)
  const maskInvalid = (): boolean => error?.type === 'spec'
  /** A row as the list shows it: under a spec error, its old name, unchanged. */
  const shownRow = (row: PreviewRow | undefined): PreviewRow | undefined =>
    row && maskInvalid() ? rowUnderBrokenMask(row) : row
  // Rows still reading, or a read that isn't done: a value may still come for some file.
  const waitsForMedia = (): boolean =>
    usesMedia() && (counts.reading > 0 || (media !== null && media.total > 0 && !media.done))
  const filterOf = (): PreviewFilter => (problemsOnly ? 'problems' : 'all')
  const listed = (): number => (problemsOnly ? counts.problems : totalOf(counts))
  const cursorIndex = (): number => (listed() === 0 ? -1 : Math.max(0, Math.min(cursor, listed() - 1)))
  const cursorRow = (): PreviewRow | undefined => shownRow(rows.get(cursorIndex()))
  // Every row of the session, problems included: what the range editor's examples come from.
  const sampleRows = $derived.by((): SampleRows | null => {
    const cursorPlace = cursorRow()?.row
    if (cursorPlace === undefined) return null
    return { cursor: cursorPlace, rows: rowsAround(cursorPlace, totalOf(counts)) }
  })

  /** Before the list's rows are replaced: with every row listed, an index is the row itself. */
  function keepCursorRow(): void {
    const row = cursorRow()?.row
    if (row === undefined) return
    if (problemsOnly) cursorAnchor = row
    else cursor = row
  }

  function specOf(id: string): MultiRenameSpec | undefined {
    return presets.find((p) => p.id === id)?.spec
  }

  function presetNamed(name: string): MultiRenamePreset | undefined {
    return presets.find((p) => nameKey(p.name) === nameKey(name))
  }

  // Read from the list, so a renamed, updated, or deleted preset is followed with no syncing.
  const loadedSpec = $derived(loadedId === null ? undefined : specOf(loadedId))

  /**
   * Previews the spec. A `quiet` one (reads landed, the spec didn't change) isn't `pending`, so
   * Rename and Results stay usable while the read goes on.
   */
  async function refresh({ quiet = false }: { quiet?: boolean } = {}): Promise<void> {
    const asked = ++generation
    if (!quiet) waiting++
    let answer
    try {
      answer = await previewMultiRename(sessionId, $state.snapshot(spec))
    } finally {
      if (!quiet) waiting--
    }
    if (asked !== generation) {
      // A newer preview landed while this one was out; with this one no longer pending, it may be the one to run on.
      runQueued()
      return
    }
    if (answer.ok) {
      keepCursorRow()
      previewId = answer.value.previewId
      counts = answer.value.counts
      // With no problems left there's nothing to list alone: every row again.
      if (counts.problems === 0 && problemsOnly) {
        problemsOnly = false
        if (cursorAnchor !== null) cursor = cursorAnchor
        cursorAnchor = null
      }
      // The first page is every row's; a list of problems pages its own in.
      rows = problemsOnly ? new SvelteMap() : new SvelteMap(answer.value.rows.map((row) => [row.row, row]))
      error = null
      show(shown)
      runQueued()
    } else {
      // A spec problem keeps the files listed (`shownRow` drops their new names) under the error.
      error = answer.error
      if (answer.error.type === 'spec' && previewId === null) void listFiles(asked)
      if (answer.error.type !== 'spec') {
        previewId = null
        counts = NO_COUNTS
        problemsOnly = false
        rows = new SvelteMap()
        // No preview to run on, and maybe no session (a closed one sends no more progress): the
        // queued Rename ends here, with the error line saying why, never hanging on "Starting…".
        if (!launching) dropQueue()
      }
    }
  }

  /**
   * The sheet opened on a spec that doesn't run, so no preview lists the files: one of the default spec
   * does, for their old names. Dropped when an edit came meanwhile.
   */
  async function listFiles(asked: number): Promise<void> {
    const answer = await previewMultiRename(sessionId, { ...DEFAULT_SPEC })
    if (asked !== generation || !answer.ok || !maskInvalid()) return
    previewId = answer.value.previewId
    counts = answer.value.counts
    rows = new SvelteMap(answer.value.rows.map((row) => [row.row, row]))
    show(shown)
  }

  function show(window: RowWindow): void {
    shown = window
    const { start, end } = window
    const id = previewId
    if (id === null) return
    const filter = filterOf()
    const last = Math.min(end, listed())
    let from = start
    while (from < last && rows.has(from)) from++
    if (from >= last) return
    let to = last
    while (to > from && rows.has(to - 1)) to--
    const limit = Math.min(to - from, PAGE_LIMIT)
    const key = `${String(id)}:${filter}:${String(from)}:${String(limit)}`
    if (fetching === key) return
    fetching = key
    const into = rows
    void getMultiRenamePreviewRows(sessionId, id, from, limit, filter).then((answer) => {
      if (fetching === key) fetching = null
      // A newer preview or the other list replaced this one: its own rows are on their way.
      if (!answer.ok || previewId !== id || rows !== into) return
      answer.value.forEach((row, i) => rows.set(from + i, row))
      if (cursorAnchor !== null) {
        const found = answer.value.findIndex((row) => row.row === cursorAnchor)
        if (found >= 0) cursor = from + found
        cursorAnchor = null
      }
      if (rows.size > HELD_ROWS) {
        const keepFrom = shown.start - PAGE_LIMIT
        const keepTo = shown.end + PAGE_LIMIT
        const far = [...rows.keys()].filter((index) => index < keepFrom || index >= keepTo)
        for (const index of far) rows.delete(index)
      }
      show(shown)
    })
  }

  async function moveRow(direction: RowMove): Promise<number | null> {
    // With every row listed, an index is the row's place in the session. Under an error the
    // list holds the last good rows, which can't show the new order.
    const from = cursorIndex()
    if (problemsOnly || error !== null || from < 0 || previewId === null) return null
    const answer = await moveMultiRenameRow(sessionId, from, direction)
    if (!answer.ok || answer.value === from) return null
    cursorAnchor = null
    cursor = answer.value
    // The move retired the preview on screen: the next one shows the new order.
    void refresh()
    return answer.value
  }

  /** Renames preview `previewId`'s ready rows; `'stillReading'` when the backend found a file still being read. */
  async function startRename(): Promise<MultiRenameStarted | 'stillReading' | null> {
    const id = previewId
    if (applying || waiting > 0 || id === null || maskInvalid()) return null
    applying = true
    try {
      // The preview the user saw: the backend renames only if it still computes exactly its ready rows.
      const answer = await applyMultiRename(sessionId, id)
      if (answer.ok) return answer.value
      if (answer.error.type === 'stillReading') return 'stillReading'
      applyError = answer.error
      // The folder moved under the preview: show it as it is now.
      if (answer.error.type === 'previewOutOfDate') void refresh()
      return null
    } finally {
      applying = false
    }
  }

  function enqueue(): Promise<MultiRenameStarted | null> {
    return new Promise((resolve) => {
      queue = resolve
      queued = true
    })
  }

  /** Ends the queue, resolving its press with `null`. */
  function dropQueue(): void {
    const resolve = queue
    queue = null
    queued = false
    resolve?.(null)
  }

  /**
   * A second press, any edit, or closing: the queued Rename won't run. Counted in `edits`, so a
   * press still waiting on the backend knows an edit came after it. A launching one can't be
   * stopped (the rename may be on its way), but it won't retry after a `stillReading`.
   */
  function cancelQueue(): void {
    edits++
    if (launching) {
      cancelRequested = true
      return
    }
    dropQueue()
  }

  /**
   * On each preview that lands: once no value can still come, the queued Rename runs on it, as a
   * fresh press would. With no preview to run on yet (the first one, or an edit's, still out), it
   * stays queued for the next.
   */
  function runQueued(): void {
    const resolve = queue
    if (resolve === null || launching || waiting > 0 || previewId === null || waitsForMedia()) return
    launching = true
    cancelRequested = false
    void startRename()
      .then((outcome) => {
        // A file still being read after all: stay queued for the next preview, unless an edit came meanwhile.
        if (outcome === 'stillReading' && !cancelRequested) return
        queue = null
        queued = false
        resolve(outcome === 'stillReading' ? null : outcome)
      })
      .finally(() => {
        launching = false
      })
  }

  function onMediaProgress(progress: MultiRenameMediaProgress): void {
    if (progress.sessionId !== sessionId || disposed) return
    media = { read: progress.read, total: progress.total, done: progress.done }
    // Rows only change with the reads when a mask names a field.
    if (!usesMedia()) return
    if (progress.done) {
      if (mediaTimer !== null) clearTimeout(mediaTimer)
      mediaTimer = null
      void refresh({ quiet: true })
      return
    }
    mediaTimer ??= setTimeout(() => {
      mediaTimer = null
      void refresh({ quiet: true })
    }, MEDIA_REFRESH_MS)
  }

  async function followMedia(): Promise<void> {
    const unlisten = await onMultiRenameMediaProgress(onMediaProgress)
    if (disposed) {
      unlisten()
      return
    }
    unlistenMedia = unlisten
    // For a read that began before the subscription; an event that came meanwhile is newer.
    const answer = await getMultiRenameMediaProgress(sessionId)
    if (answer.ok && media === null && !isDisposed()) media = answer.value
  }

  /** The files at the list's window are read next, while the read goes on. */
  function prioritizeShown({ start, end }: RowWindow): void {
    if (media !== null && (media.done || media.total === 0)) return
    const last = Math.min(end, listed())
    const inView: number[] = []
    for (let index = start; index < last; index++) {
      const place = problemsOnly ? rows.get(index)?.row : index
      if (place !== undefined) inView.push(place)
    }
    const key = inView.join(',')
    if (inView.length === 0 || key === prioritized) return
    prioritized = key
    void prioritizeMultiRenameMedia(sessionId, inView)
  }

  function schedule(): void {
    cancelQueue()
    applyError = null
    if (timer === null) waiting++
    else clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      waiting--
      void refresh()
    }, PREVIEW_DELAY_MS)
  }

  // The first preview, on the settings the last sheet closed with (or the defaults).
  async function start(): Promise<void> {
    let last = null
    try {
      last = await getMultiRenameLastSettings()
    } catch {
      // Nothing remembered: the defaults.
    }
    if (last && !touched) {
      spec = { ...DEFAULT_SPEC, ...last.spec }
      loadedId = last.presetId
    }
    await refresh()
  }
  void start()
  void followMedia()

  return {
    get spec() {
      return spec
    },
    get counts() {
      return counts
    },
    get total() {
      return listed()
    },
    get problemsOnly() {
      return problemsOnly
    },
    source: {
      get count() {
        return listed()
      },
      getRow: (index) => shownRow(rows.get(index)),
      onRangeChange: (range) => {
        show(range)
        prioritizeShown(range)
      },
    },
    get error() {
      return error
    },
    get maskInvalid() {
      return maskInvalid()
    },
    get syntaxSwitched() {
      return syntaxSwitched
    },
    undoSyntaxSwitch() {
      touched = true
      spec = { ...spec, maskSyntax: 'cmdr' }
      syntaxSwitched = false
      schedule()
    },
    get presets() {
      return presets
    },
    get loadedId() {
      return loadedSpec ? loadedId : null
    },
    get edited() {
      return loadedSpec !== undefined && !specsEqual(spec, loadedSpec)
    },
    get applying() {
      return applying
    },
    get applyError() {
      return applyError
    },
    get editedCount() {
      return editedCount
    },
    get cursorIndex() {
      return cursorIndex()
    },
    get cursorRow() {
      return cursorRow()
    },
    moveCursorRow(direction) {
      cancelQueue()
      // Pending from the key on, so a Start right after it waits for the new order's preview.
      waiting++
      const next = moving
        .then(() => moveRow(direction))
        .finally(() => {
          waiting--
        })
      moving = next.catch(() => {})
      return next
    },
    setCursor(index) {
      cursorAnchor = null
      cursor = Math.max(0, Math.min(index, listed() - 1))
    },
    get sampleRows() {
      return sampleRows
    },
    async rangeSamples(token, sampleRowList) {
      const answer = await getMultiRenameRangeSamples(sessionId, token, sampleRowList)
      return answer.ok ? answer.value : null
    },
    get pending() {
      return waiting > 0
    },
    rowAt(index) {
      return shownRow(rows.get(index))
    },
    show,
    setProblemsOnly(on) {
      if (on === problemsOnly) return
      // The problems start at the top; every row again goes back to the cursor's own row.
      const row = cursorRow()?.row ?? cursorAnchor
      cursorAnchor = null
      cursor = on ? 0 : (row ?? 0)
      problemsOnly = on
      rows = new SvelteMap()
      fetching = null
      show(shown)
    },
    update(patch) {
      touched = true
      const next = patchSpec(spec, patch)
      spec = next.spec
      if (next.switched !== null) syntaxSwitched = next.switched
      schedule()
    },
    loadPreset(id) {
      const next = specOf(id)
      if (!next) return
      touched = true
      loadedId = id
      spec = { ...next }
      syntaxSwitched = false
      schedule()
    },
    resetFields() {
      touched = true
      loadedId = null
      spec = { ...DEFAULT_SPEC }
      syntaxSwitched = false
      schedule()
    },
    async loadPresets() {
      presets = await getMultiRenamePresets(
        PRESET_SEEDS.map((seed) => ({ name: tString(seed.nameKey), spec: seed.spec })),
      )
    },
    presetNamed,
    async savePreset(name) {
      const trimmed = name.trim()
      if (trimmed === '') return
      const id = presetNamed(trimmed)?.id ?? crypto.randomUUID()
      presets = await saveMultiRenamePreset({ id, name: trimmed, spec: $state.snapshot(spec) })
      loadedId = id
    },
    async renamePreset({ id, name }) {
      const trimmed = name.trim()
      if (trimmed === '') return
      presets = await renameMultiRenamePreset(id, trimmed)
    },
    async updatePreset(id) {
      presets = await updateMultiRenamePreset(id, $state.snapshot(spec))
      loadedId = id
    },
    async movePreset({ id, direction }) {
      presets = await moveMultiRenamePreset(id, direction)
    },
    async deletePreset(id) {
      presets = await deleteMultiRenamePreset(id)
      if (loadedId === id) loadedId = null
    },
    async persist() {
      // A preset deleted meanwhile isn't remembered: `loadedId` reads it from the list.
      await saveMultiRenameLastSettings($state.snapshot(spec), loadedSpec ? loadedId : null)
    },
    async writeNames() {
      const id = previewId
      if (waiting > 0 || id === null || maskInvalid()) return null
      const answer = await writeMultiRenameNames(sessionId, id)
      if (answer.ok) return answer.value
      applyError = answer.error
      return null
    },
    async readNames() {
      const answer = await readMultiRenameNames(sessionId)
      if (!answer.ok) return answer.error
      editedCount = answer.value
      schedule()
      return null
    },
    async clearNames() {
      editedCount = 0
      await clearMultiRenameNames(sessionId)
      schedule()
    },
    async apply() {
      if (queue !== null) {
        cancelQueue()
        return null
      }
      if (maskInvalid()) return null
      if (waitsForMedia()) return enqueue()
      const pressedAt = edits
      const outcome = await startRename()
      if (outcome !== 'stillReading') return outcome
      // The backend found a file still being read: wait for it, as a press during the read does,
      // unless an edit came while the press was out (no press was made for the edited spec).
      return pressedAt === edits ? enqueue() : null
    },
    get media() {
      return media
    },
    get usesMedia() {
      return usesMedia()
    },
    get waitsForMedia() {
      return waitsForMedia()
    },
    get queued() {
      return queued
    },
    get launching() {
      return launching
    },
    async mediaOverview() {
      const answer = await getMultiRenameMediaOverview(sessionId)
      return answer.ok ? answer.value : null
    },
    async fieldSamples(token, sampleRowList) {
      const answer = await getMultiRenameFieldSamples(sessionId, token, sampleRowList)
      return answer.ok ? answer.value : null
    },
    dispose() {
      disposed = true
      if (timer !== null) clearTimeout(timer)
      if (mediaTimer !== null) clearTimeout(mediaTimer)
      unlistenMedia?.()
      cancelQueue()
      generation++
    },
  }
}
