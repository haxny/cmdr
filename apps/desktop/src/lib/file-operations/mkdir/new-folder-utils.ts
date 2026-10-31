import { nameWithoutExtension } from '#lib/utils/file-extension.js'

/** Derives a folder name suggestion from a filename by removing its extension (the product's one rule). */
export function removeExtension(filename: string): string {
  return nameWithoutExtension(filename, 'file')
}
