/**
 * The Multi-Rename sheet's state: the spec the fields edit, the live preview, the
 * presets, the names typed in Results, and Start. One instance per open sheet.
 *
 * It opens on the settings the last sheet closed with (`persist`), as TC does.
 *
 * The preview reruns 120 ms after the last edit, and a generation counter drops
 * an answer a newer edit overtook, so fast typing never shows an older preview.
 * The names are the backend's (`src-tauri/src/multi_rename/`): the sheet sends
 * the spec and the pane's rows, never names.
 */

import {
  applyMultiRename,
  deleteMultiRenamePreset,
  getMultiRenameHistory,
  getMultiRenameLastSpec,
  getMultiRenamePresets,
  previewMultiRename,
  readMultiRenameNames,
  saveMultiRenameLastSpec,
  saveMultiRenamePreset,
  writeMultiRenameNames,
  type FieldHistoryEntry,
  type HistoryField,
  type MultiRenameError,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type NameEdit,
  type PreviewRow,
} from '$lib/tauri-commands'
import { DEFAULT_SPEC, completeSpec, countPreview, type PreviewCounts } from './spec'

/** What the sheet renames: a pane's listing and its selected rows (backend numbers), or all of them. */
export interface MultiRenameTarget {
  listingId: string
  includeHidden: boolean
  /** Backend row numbers in rename order, or `null` for every row the pane shows. */
  rows: number[] | null
}

export const PREVIEW_DELAY_MS = 120

export interface MultiRenameState {
  readonly spec: MultiRenameSpec
  readonly rows: PreviewRow[]
  readonly counts: PreviewCounts
  /** Why the preview has no answer: a bad mask or regex, a gone listing, a timeout. */
  readonly error: MultiRenameError | null
  /** Why the last Start didn't run. Cleared by the next edit or preview; doesn't block a retry. */
  readonly applyError: MultiRenameError | null
  /** An edit the preview hasn't caught up with yet: Start waits, so it never runs a spec nobody saw. */
  readonly pending: boolean
  readonly presets: MultiRenamePreset[]
  readonly applying: boolean
  /** What each text field held when renames ran, newest first (⌥⇧↓). */
  historyOf: (field: HistoryField) => string[]
  loadHistory: () => Promise<void>
  /** Names the user typed in Results, by old name. Empty until they read some back. */
  readonly edits: NameEdit[]
  update: (patch: Partial<MultiRenameSpec>) => void
  /** Replaces the whole spec (loading a preset). */
  load: (spec: MultiRenameSpec) => void
  loadPresets: () => Promise<void>
  savePreset: (name: string) => Promise<void>
  deletePreset: (id: string) => Promise<void>
  /** Results: writes the preview to a text file. Its path, or `null` when it couldn't (`applyError` says why). */
  writeNames: () => Promise<string | null>
  /** Reads the Results file back into `edits`. False when there was nothing to read. */
  readNames: () => Promise<boolean>
  /** Drops the typed names: every row shows its computed name again. */
  discardEdits: () => void
  /** Remembers the settings for the next sheet. */
  persist: () => Promise<void>
  /** Starts the rename. Resolves with the operation, or `null` when it didn't start (`error` says why). */
  apply: () => Promise<MultiRenameStarted | null>
  dispose: () => void
}

export function createMultiRenameState(target: MultiRenameTarget): MultiRenameState {
  let spec = $state<MultiRenameSpec>({ ...DEFAULT_SPEC })
  let edits = $state.raw<NameEdit[]>([])
  // An edit made before the last settings arrive wins over them.
  let touched = false
  let rows = $state.raw<PreviewRow[]>([])
  let error = $state<MultiRenameError | null>(null)
  let presets = $state.raw<MultiRenamePreset[]>([])
  let history = $state.raw<FieldHistoryEntry[]>([])
  let applying = $state(false)
  let applyError = $state<MultiRenameError | null>(null)
  let waiting = $state(0)
  let generation = 0
  let timer: ReturnType<typeof setTimeout> | null = null

  async function refresh(): Promise<void> {
    const asked = ++generation
    waiting++
    let answer
    try {
      answer = await previewMultiRename(
        target.listingId,
        target.includeHidden,
        target.rows,
        $state.snapshot(spec),
        edits,
      )
    } finally {
      waiting--
    }
    if (asked !== generation) return
    if (answer.ok) {
      rows = answer.value
      error = null
    } else {
      // A spec problem keeps the last good preview on screen under the error.
      error = answer.error
      if (answer.error.type !== 'spec') rows = []
    }
  }

  function schedule(): void {
    applyError = null
    if (timer === null) waiting++
    else clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      waiting--
      void refresh()
    }, PREVIEW_DELAY_MS)
  }

  // The first preview, on the settings the last sheet closed with.
  async function start(): Promise<void> {
    let last: MultiRenameSpec | null = null
    try {
      last = await getMultiRenameLastSpec()
    } catch {
      // No remembered settings: the default ones.
    }
    if (last && !touched) spec = completeSpec(last)
    await refresh()
  }
  void start()

  return {
    get spec() {
      return spec
    },
    get rows() {
      return rows
    },
    get counts() {
      return countPreview(rows)
    },
    get error() {
      return error
    },
    get presets() {
      return presets
    },
    get applying() {
      return applying
    },
    get applyError() {
      return applyError
    },
    get edits() {
      return edits
    },
    historyOf(field) {
      return history.filter((h) => h.field === field).map((h) => h.value)
    },
    async loadHistory() {
      history = await getMultiRenameHistory()
    },
    get pending() {
      return waiting > 0
    },
    update(patch) {
      touched = true
      spec = { ...spec, ...patch }
      schedule()
    },
    load(next) {
      touched = true
      spec = completeSpec(next)
      schedule()
    },
    async loadPresets() {
      presets = await getMultiRenamePresets()
    },
    async savePreset(name) {
      const trimmed = name.trim()
      if (trimmed === '') return
      const existing = presets.find((p) => p.name.trim().toLowerCase() === trimmed.toLowerCase())
      await saveMultiRenamePreset({
        id: existing?.id ?? crypto.randomUUID(),
        name: trimmed,
        spec: $state.snapshot(spec),
      })
      presets = await getMultiRenamePresets()
    },
    async deletePreset(id) {
      await deleteMultiRenamePreset(id)
      presets = await getMultiRenamePresets()
    },
    async writeNames() {
      const answer = await writeMultiRenameNames(
        target.listingId,
        target.includeHidden,
        target.rows,
        $state.snapshot(spec),
        edits,
      )
      if (answer.ok) return answer.value
      applyError = answer.error
      return null
    },
    async readNames() {
      const answer = await readMultiRenameNames()
      if (!answer.ok) return false
      edits = answer.value
      schedule()
      return true
    },
    discardEdits() {
      edits = []
      schedule()
    },
    async persist() {
      await saveMultiRenameLastSpec($state.snapshot(spec))
    },
    async apply() {
      if (applying || waiting > 0) return null
      applying = true
      try {
        // What the user saw: the backend renames only if it still computes exactly this.
        const expected = rows
          .filter((r) => r.status.type === 'ready')
          .map((r) => ({ row: r.row, oldName: r.oldName, newName: r.newName }))
        const answer = await applyMultiRename(
          target.listingId,
          target.includeHidden,
          target.rows,
          $state.snapshot(spec),
          edits,
          expected,
        )
        if (answer.ok) return answer.value
        applyError = answer.error
        // The folder moved under the preview: show it as it is now.
        if (answer.error.type === 'previewOutOfDate') void refresh()
        return null
      } finally {
        applying = false
      }
    },
    dispose() {
      if (timer !== null) clearTimeout(timer)
      generation++
    },
  }
}
