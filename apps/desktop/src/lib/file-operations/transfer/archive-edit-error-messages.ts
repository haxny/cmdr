/**
 * The friendly messages for an edit INSIDE a zip that Cmdr refuses: each names
 * the entry (and the zip) at fault, since it's rarely the one the user touched.
 * Composed like the rest of `transfer-error-messages.ts`, which calls in here.
 */
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { escapeHtml } from '#lib/tooltip/tooltip.js'
import { w, type FriendlyErrorMessage } from './transfer-error-catalog.js'

/** The message for a refused zip edit, or `null` for any other error. */
export function archiveEditMessage(error: WriteOperationError): FriendlyErrorMessage | null {
  switch (error.type) {
    // A zip entry the archive reader would hide. Named, because one entry deep
    // in a selected folder is the whole problem, and the reason picks the
    // sentence since each one needs a different picture of what's wrong.
    case 'archive_entry_name_refused':
      return {
        title: w('archiveEntryNameRefused.title'),
        message: w(`archiveEntryNameRefused.message.${error.reason}`, { entry: escapeHtml(error.entry) }),
        suggestion: w('archiveEntryNameRefused.suggestion'),
      }
    // Both names, plus where they'd meet: the backslash look-alike is the usual
    // cause, and neither name alone shows the clash.
    case 'archive_entry_names_collide':
      return {
        title: w('archiveEntryNamesCollide.title'),
        message: w('archiveEntryNamesCollide.message', {
          entry: escapeHtml(error.entry),
          other: escapeHtml(error.other),
          archivePath: escapeHtml(error.archivePath),
        }),
        suggestion: w('archiveEntryNamesCollide.suggestion'),
      }
    // Both the zip and the shared name: the name alone doesn't say which zip,
    // and Cmdr's own listing shows only one of the two. The name is null when
    // Cmdr's reader couldn't parse the zip, so a sentence without it stands in.
    case 'archive_duplicate_entry_name':
      return {
        title: w('archiveDuplicateEntryName.title'),
        message:
          error.name === null
            ? w('archiveDuplicateEntryName.messageUnnamed', { path: escapeHtml(error.path) })
            : w('archiveDuplicateEntryName.message', { name: escapeHtml(error.name), path: escapeHtml(error.path) }),
        suggestion: w('archiveDuplicateEntryName.suggestion'),
      }
    // Named, because the protected entry is rarely the one the user touched.
    case 'archive_encrypted_entry':
      return {
        title: w('archiveEncryptedEntry.title'),
        message: w('archiveEncryptedEntry.message', {
          name: escapeHtml(error.name),
          path: escapeHtml(error.path),
        }),
        suggestion: w('archiveEncryptedEntry.suggestion'),
      }
    // Both names, since the clash is between two entries of one zip.
    case 'archive_entry_under_file':
      return {
        title: w('archiveEntryUnderFile.title'),
        message: w('archiveEntryUnderFile.message', {
          entry: escapeHtml(error.entry),
          file: escapeHtml(error.file),
          path: escapeHtml(error.path),
        }),
        suggestion: w('archiveEntryUnderFile.suggestion'),
      }
    default:
      return null
  }
}
