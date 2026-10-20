import { commands, type PastedClipboardFile } from '#lib/ipc/bindings.js'
import { throwIpcError } from './ipc-types'
import { awaitClipboardPaste, type MutationWaitOptions } from './mutation-reply'

export type { PastedClipboardFile } from '#lib/ipc/bindings.js'

export interface ClipboardReadResult {
  paths: string[]
  isCut: boolean
  /** Per-path top-level kind, index-aligned with `paths`: `true` = directory,
   *  `false` = file, `null` = unknown (stat failed). Lets the paste completion
   *  toast split files vs. folders without walking trees. */
  isDirectory: (boolean | null)[]
}

/**
 * Writes the given paths to the system clipboard as a copy. Paths by value: a
 * pane's selection is resolved through the revision-checked
 * `getSelectionSnapshot` first, never sent as row numbers.
 */
export async function copyPathsToClipboard(paths: string[]): Promise<number> {
  const res = await commands.copyPathsToClipboard(paths)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Writes the given paths to the system clipboard and marks them as cut. */
export async function cutPathsToClipboard(paths: string[]): Promise<number> {
  const res = await commands.cutPathsToClipboard(paths)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function readClipboardFiles(): Promise<ClipboardReadResult> {
  const res = await commands.readClipboardFiles()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function readClipboardText(): Promise<string | null> {
  const res = await commands.readClipboardText()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

export async function clearClipboardCutState(): Promise<void> {
  await commands.clearClipboardCutState()
}

/**
 * Reads the highest-intent non-file clipboard flavor (image / PDF / text) and
 * writes it into `directory` as a new `pasted.<ext>` file, returning the created
 * file's name + kind once it landed, however slow the volume is
 * (`wait.onStillRunning` says when it's being slow, `./mutation-reply.ts`).
 * Resolves to `null` when nothing pasteable is on the clipboard — the caller
 * treats that as "no file created" (today's warn toast), not an error. A refusal
 * throws typed (`MutationFailure`).
 */
export async function pasteClipboardAsFile(
  volumeId: string | null,
  directory: string,
  wait?: MutationWaitOptions,
): Promise<PastedClipboardFile | null> {
  return awaitClipboardPaste(() => commands.pasteClipboardAsFile(volumeId, directory), wait)
}
