/**
 * Audiobookshelf's side files (`reader.txt`, `desc.txt`) in the Write dialog's list and in ⌘⏎'s write, behind the
 * `multiRename.tagEditor.absSideFiles` setting. The backend decides what each says and guards the write
 * (`tag_editor/side_files.rs`); this words the list and hands back which replacements the list showed. Pure but for
 * the setting read.
 */
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { SideFile, SideFileWithheld } from '#lib/ipc/bindings.js'
import { getSetting } from '#lib/settings/index.js'
import type { SideFileChange, SideFileSeen } from '#lib/tauri-commands/index.js'

/** Whether the user asked for the side files (off by default: a write then touches only the audio files). */
export function sideFilesOn(): boolean {
  return getSetting('multiRename.tagEditor.absSideFiles')
}

export const SIDE_FILE_NAMES: Readonly<Record<SideFile, string>> = {
  reader: 'reader.txt',
  desc: 'desc.txt',
}

/** The longest old or new text a list row quotes; the write compares the full text. */
export const SIDE_TEXT_SHOWN = 200

/** `text` as a list row quotes it: one line, at most `SIDE_TEXT_SHOWN` characters. */
export function quoted(text: string): string {
  const line = text.trim().replace(/\s+/g, ' ')
  return line.length > SIDE_TEXT_SHOWN ? `${line.slice(0, SIDE_TEXT_SHOWN)}…` : line
}

function whyKey(file: SideFile, reason: SideFileWithheld): MessageKey {
  switch (reason) {
    case 'tracksDisagree':
      return file === 'reader'
        ? 'multiRename.tags.write.sideWhy.readerDisagree'
        : 'multiRename.tags.write.sideWhy.descDisagree'
    case 'tooLong':
      return 'multiRename.tags.write.sideWhy.tooLong'
    case 'notAFile':
      return 'multiRename.tags.write.sideWhy.notAFile'
    case 'unreadable':
      return 'multiRename.tags.write.sideWhy.unreadable'
  }
}

/** One list row, as a message and its params; a withheld file's reason is a message of its own. */
export type SideRow =
  | { key: MessageKey; params: Record<string, string> }
  | { key: MessageKey; params: { fileName: string }; why: MessageKey }

export function sideRowOf(change: SideFileChange): SideRow {
  const fileName = SIDE_FILE_NAMES[change.file]
  switch (change.kind) {
    case 'create':
      return { key: 'multiRename.tags.write.sideCreate', params: { fileName, text: quoted(change.text) } }
    case 'replace':
      return {
        key: 'multiRename.tags.write.sideReplace',
        params: { fileName, old: quoted(change.old), text: quoted(change.text) },
      }
    case 'withheld':
      return {
        key: 'multiRename.tags.write.sideWithheld',
        params: { fileName },
        why: whyKey(change.file, change.reason),
      }
  }
}

/** Whether a write from this list would write a side file (a withheld one is only explained). */
export function sideWrites(changes: readonly SideFileChange[]): boolean {
  return changes.some((change) => change.kind !== 'withheld')
}

/** The replacements this list showed, each with the old text it showed: the only ones its write may make. */
export function replacementsShown(changes: readonly SideFileChange[]): SideFileSeen[] {
  return changes.flatMap((change) => (change.kind === 'replace' ? [{ path: change.path, old: change.old }] : []))
}

/** How many side files the list would replace: ⌘⏎ shows those first. */
export function replacing(changes: readonly SideFileChange[]): number {
  return changes.filter((change) => change.kind === 'replace').length
}
