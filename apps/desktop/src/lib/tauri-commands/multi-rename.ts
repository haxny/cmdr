// Multi-Rename Tool (⌃M): preview, apply, Results, presets, and the last settings. The work is the backend's
// (`src-tauri/src/multi_rename/`); these are pass-throughs.

import {
  commands,
  type ExpectedRename,
  type MultiRenameError,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type NameEdit,
  type NamesFileError,
  type PreviewRow,
} from '$lib/ipc/bindings'

export type {
  ExpectedRename,
  MultiRenameError,
  MultiRenamePreset,
  MultiRenameSpec,
  MultiRenameStarted,
  NameEdit,
  NamesFileError,
  PreviewRow,
}

/** A typed answer the sheet words itself: the rows, or why there are none. */
export type MultiRenameResult<T> = { ok: true; value: T } | { ok: false; error: MultiRenameError }

/**
 * Each row's new name and whether it can take it. `rows` are backend row numbers
 * in rename order; `null` previews every row the pane shows. `edits` are names
 * the user typed in Results, by old name.
 */
export async function previewMultiRename(
  listingId: string,
  includeHidden: boolean,
  rows: number[] | null,
  spec: MultiRenameSpec,
  edits: NameEdit[],
): Promise<MultiRenameResult<PreviewRow[]>> {
  const res = await commands.previewMultiRename(listingId, includeHidden, rows, spec, edits)
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/**
 * Renames the rows the user saw as ready (`expected`), as one operation (queue,
 * Undo). `previewOutOfDate` when the folder changed since that preview.
 */
export async function applyMultiRename(
  listingId: string,
  includeHidden: boolean,
  rows: number[] | null,
  spec: MultiRenameSpec,
  edits: NameEdit[],
  expected: ExpectedRename[],
): Promise<MultiRenameResult<MultiRenameStarted>> {
  const res = await commands.applyMultiRename(listingId, includeHidden, rows, spec, edits, expected)
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/** Results (⌥⏎): writes the preview as `old<TAB>new` lines and returns the file's path, for the editor. */
export async function writeMultiRenameNames(
  listingId: string,
  includeHidden: boolean,
  rows: number[] | null,
  spec: MultiRenameSpec,
  edits: NameEdit[],
): Promise<MultiRenameResult<string>> {
  const res = await commands.writeMultiRenameNames(listingId, includeHidden, rows, spec, edits)
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/** The names the user typed in the Results file, by old name. */
export async function readMultiRenameNames(): Promise<
  { ok: true; value: NameEdit[] } | { ok: false; error: NamesFileError }
> {
  const res = await commands.readMultiRenameNames()
  return res.status === 'ok' ? { ok: true, value: res.data } : { ok: false, error: res.error }
}

/** The settings the sheet last closed with, or `null` before the first close. */
export async function getMultiRenameLastSpec(): Promise<MultiRenameSpec | null> {
  return commands.getMultiRenameLastSpec()
}

/** Remembers the settings the sheet closes with, for the next ⌃M. */
export async function saveMultiRenameLastSpec(spec: MultiRenameSpec): Promise<void> {
  await commands.saveMultiRenameLastSpec(spec)
}

export async function getMultiRenamePresets(): Promise<MultiRenamePreset[]> {
  return commands.getMultiRenamePresets()
}

export async function saveMultiRenamePreset(preset: MultiRenamePreset): Promise<void> {
  await commands.saveMultiRenamePreset(preset)
}

export async function deleteMultiRenamePreset(id: string): Promise<void> {
  await commands.deleteMultiRenamePreset(id)
}
