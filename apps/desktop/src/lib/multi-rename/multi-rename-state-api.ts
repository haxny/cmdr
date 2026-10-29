/**
 * The Multi-Rename sheet state's shape (`MultiRenameState`, made by `createMultiRenameState` in
 * `multi-rename-state.svelte.ts`), the list window and the range editor's sample rows, and what a row
 * shows under a broken mask. Pure.
 */
import type {
  FieldSamples,
  MediaOverview,
  MediaProgress,
  MultiRenameError,
  MultiRenamePreset,
  MultiRenameSpec,
  MultiRenameStarted,
  PresetMove,
  PreviewCounts,
  PreviewRow,
  RangeSample,
  RowMove,
} from '#lib/tauri-commands/index.js'
import type { ColumnListWindowedSource } from '#lib/ui/column-list-types.js'

/**
 * A row as the list shows it while a mask or the search doesn't parse: its old name, unchanged, never
 * the last good preview's new name (half a mask's names read as real ones).
 */
export function rowUnderBrokenMask(row: PreviewRow): PreviewRow {
  return { ...row, newName: row.oldName, status: { type: 'unchanged' }, edited: false }
}

/** How many files the range editor shows its range on. */
const SAMPLE_ROWS = 5

/** Rows `start..end` of the preview, end exclusive. */
export interface RowWindow {
  start: number
  end: number
}

/**
 * The session rows around `cursor` the range editor shows: up to `SAMPLE_ROWS`, the cursor's
 * neighbors on both sides, shifted at either end of the `total` rows so it stays that many.
 */
export function rowsAround(cursor: number, total: number): number[] {
  const count = Math.min(SAMPLE_ROWS, total)
  const start = Math.max(0, Math.min(cursor - Math.floor(SAMPLE_ROWS / 2), total - count))
  return Array.from({ length: count }, (_, i) => start + i)
}

/** The rows the range editor shows its range on: the cursor's session row and the rows around it. */
export interface SampleRows {
  cursor: number
  rows: number[]
}

export interface MultiRenameState {
  readonly spec: MultiRenameSpec
  readonly counts: PreviewCounts
  /** How many rows the list shows: the preview's, or its problem rows' with `problemsOnly`. */
  readonly total: number
  /** The list shows the problem rows alone. */
  readonly problemsOnly: boolean
  /** The rows as `ColumnList` reads them: `total`, `rowAt`, and `show`. */
  readonly source: ColumnListWindowedSource<PreviewRow>
  /** Why the preview has no answer: a bad mask or regex, a gone listing, a timeout. */
  readonly error: MultiRenameError | null
  /** A spec error: rows show their old names (never the last good preview's) and Rename and Results don't run. */
  readonly maskInvalid: boolean
  /** The name mask switched to foobar on its own, as a foobar pattern came in: the sheet says so, with a way back. */
  readonly syntaxSwitched: boolean
  /** Back to Cmdr syntax after an automatic switch; the mask's text stays. */
  undoSyntaxSwitch: () => void
  /** Why the last Start didn't run. Cleared by the next edit or preview; doesn't block a retry. */
  readonly applyError: MultiRenameError | null
  /** An edit the preview hasn't caught up with yet: Start waits, so it never runs a spec nobody saw. */
  readonly pending: boolean
  /** The presets, top to bottom: the menu numbers them by position. */
  readonly presets: MultiRenamePreset[]
  /** The id of the preset the fields came from, or `null` (nothing loaded, Reset all fields, or it was deleted). */
  readonly loadedId: string | null
  /** A field differs from the loaded preset. Always `false` with nothing loaded. */
  readonly edited: boolean
  readonly applying: boolean
  /** How many rows carry a name the user typed in Results; every preview from now on shows them. */
  readonly editedCount: number
  /** The list's cursor row, an index like `rowAt`'s, or -1 with nothing listed. */
  readonly cursorIndex: number
  /** The cursor row, or `undefined` while its page is on its way (or nothing is listed). */
  readonly cursorRow: PreviewRow | undefined
  /** Puts the cursor on row `index` of the list, clamped to it. */
  setCursor: (index: number) => void
  /**
   * The session rows the range editor shows (the cursor row and up to four around it, by place
   * in the rename order), or `null` with no cursor row yet.
   */
  readonly sampleRows: SampleRows | null
  /**
   * What the range placeholder `token` (`N2-5`) takes from the session files at `rows`: the
   * engine's answer, or `null` when it can't say (a moved row's preview still on its way).
   */
  rangeSamples: (token: string, rows: number[]) => Promise<RangeSample[] | null>
  /**
   * Moves the cursor row one place in the rename order (what `[C]` counts), the cursor with
   * it, and previews again. Resolves with its new index, or `null` when it didn't move: at
   * that end, with the problems listed alone (a move among them is ambiguous), under a preview
   * error (the rows on screen can't show the new order), or refused.
   */
  moveCursorRow: (direction: RowMove) => Promise<number | null>
  /** Row `index` of the list (of the problem rows, with `problemsOnly`), or `undefined` while its page is on its way. */
  rowAt: (index: number) => PreviewRow | undefined
  /** The list shows rows `start..end`: fetch the ones missing, drop the far ones. */
  show: (window: RowWindow) => void
  /** Lists the problem rows alone, or every row again. Pages them in from the same preview. */
  setProblemsOnly: (on: boolean) => void
  update: (patch: Partial<MultiRenameSpec>) => void
  /** Fills every field from preset `id` (doesn't start anything). An unknown one is ignored. */
  loadPreset: (id: string) => void
  /** Total Commander's `<Default>`: every field back to "no change", nothing loaded. */
  resetFields: () => void
  /** Reads the list. One that never took the seeds gets them first, named in the UI language (the backend decides). */
  loadPresets: () => Promise<void>
  /** The saved preset the backend would treat as `name` (trimmed, any case), if any. */
  presetNamed: (name: string) => MultiRenamePreset | undefined
  /** Saves the fields under `name`, replacing a preset with that name; it becomes the loaded one. */
  savePreset: (name: string) => Promise<void>
  /** Renames a preset in place; a preset with that name is replaced. */
  renamePreset: (args: { id: string; name: string }) => Promise<void>
  /** Gives a preset the current fields; it becomes the loaded one. */
  updatePreset: (id: string) => Promise<void>
  /** Move up / Move down: swaps a preset with its neighbor, so the menu's numbers follow. */
  movePreset: (args: { id: string; direction: PresetMove }) => Promise<void>
  deletePreset: (id: string) => Promise<void>
  /** Remembers the fields and the loaded preset for the next sheet. */
  persist: () => Promise<void>
  /** Results: writes the shown preview's names to a file and resolves with its path, or `null` (`applyError` says why). */
  writeNames: () => Promise<string | null>
  /** Reads the Results file back and re-previews. Resolves with why it couldn't, or `null`. */
  readNames: () => Promise<MultiRenameError | null>
  /** Drops the typed names: every row follows the settings again. */
  clearNames: () => Promise<void>
  /**
   * Rename. Resolves with the operation, or `null` when it didn't start (`applyError` says why).
   * While `waitsForMedia`, it queues instead: it resolves once the read is done and the rename of
   * the final preview started, or with `null` when cancelled (a second call, any edit, or closing).
   */
  apply: () => Promise<MultiRenameStarted | null>
  /** How far the session's read of its files' EXIF and tags got; `null` until known. */
  readonly media: MediaProgress | null
  /** A mask uses a media field (`[=exif.taken]`, …). */
  readonly usesMedia: boolean
  /** Rename would wait for the read: a mask uses a media field, and some file's fields are still on their way. */
  readonly waitsForMedia: boolean
  /** A Rename is waiting for the read ("Starting…"). */
  readonly queued: boolean
  /** The queued Rename is on its way to the backend: too late to cancel. */
  readonly launching: boolean
  /** The field picker's overview: each field's example from the session's files; `null` when it can't say. */
  mediaOverview: () => Promise<MediaOverview | null>
  /**
   * The media placeholder `token` (`=exif.taken:d`) alone on the session files at `rows`, and how it
   * fares across them all; `null` when it can't say.
   */
  fieldSamples: (token: string, rows: number[]) => Promise<FieldSamples | null>
  dispose: () => void
}
