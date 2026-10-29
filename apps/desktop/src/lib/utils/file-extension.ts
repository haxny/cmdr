/**
 * What counts as a name's extension: the frontend mirror of the product's ONE
 * rule, `crates/cmdr-fs/src/file_extension.rs`. The rule, why it's mirrored
 * rather than asked over IPC, and who uses it: `crates/cmdr-fs/DETAILS.md` §
 * "What counts as an extension". Everything on the frontend that splits a name
 * at its extension goes through here. `file-extension.test.ts` runs the
 * backend's own cases (`file_extension_cases.json`) against it.
 *
 * In short: a folder has none unless it's a macOS package (`Cmdr.app`); a
 * file's is 1–10 ASCII letters and digits after the last dot, or a known
 * compound (`tar.gz`); a leading or trailing dot isn't one.
 */

/** Folder suffixes that make macOS present the folder as one item. Lowercase, no dot. */
export const PACKAGE_EXTENSIONS: readonly string[] = [
  'app',
  'appex',
  'bundle',
  'component',
  'framework',
  'kext',
  'mdimporter',
  'photoslibrary',
  'pkg',
  'plugin',
  'prefpane',
  'qlgenerator',
  'rtfd',
  'saver',
  'wdgt',
  'xcodeproj',
  'xcworkspace',
]

/** Two-part extensions that count as one: the compressed tars. Lowercase, no leading dot. */
export const COMPOUND_EXTENSIONS: readonly string[] = ['tar.gz', 'tar.bz2', 'tar.xz', 'tar.zst']

const PLAIN_EXTENSION = /^[A-Za-z0-9]{1,10}$/

/** The part after the last dot, when there's a stem before it. */
function lastSegment(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1) : ''
}

function compoundExtension(name: string): string {
  const lower = name.toLowerCase()
  for (const compound of COMPOUND_EXTENSIONS) {
    // A real stem before the dot, so `.tar.gz` alone is a dotfile's `gz`.
    if (lower.length > compound.length + 1 && lower.endsWith(`.${compound}`)) {
      return name.slice(name.length - compound.length)
    }
  }
  return ''
}

/**
 * The extension of `name`, without its dot and in the name's own case, or `''`
 * when it has none under the product's rule.
 */
export function fileExtension(name: string, isDirectory: boolean): string {
  const last = lastSegment(name)
  if (isDirectory) return PACKAGE_EXTENSIONS.includes(last.toLowerCase()) ? last : ''
  const compound = compoundExtension(name)
  if (compound) return compound
  return PLAIN_EXTENSION.test(last) ? last : ''
}

/** `name` with its extension and the dot before it stripped, or the whole name when it has none. */
export function nameWithoutExtension(name: string, isDirectory: boolean): string {
  const ext = fileExtension(name, isDirectory)
  return ext ? name.slice(0, name.length - ext.length - 1) : name
}

/** `name` with its extension replaced by `extension` (no dot; `''` removes it), or added when it had none. */
export function withExtension(name: string, extension: string, isDirectory: boolean): string {
  const stem = nameWithoutExtension(name, isDirectory)
  return extension ? `${stem}.${extension}` : stem
}
