/**
 * Classifies a file by extension to decide whether the file viewer should
 * show the "this is the raw view" banner.
 *
 * The file viewer renders bytes (lossy UTF-8). For images, PDFs, archives,
 * media etc. that's almost never what the user wanted — they probably hit
 * F3 expecting a Finder-style preview. The banner explains the difference
 * and nudges them toward ⇧Space (Quick Look) or Enter (open in associated
 * app).
 *
 * Approach: explicit allow-lists per category. Anything not on a list (no
 * extension, source code, configs, logs, CSV, markdown, SVG, etc.) does
 * NOT trigger the banner. This is conservative — better to under-warn than
 * over-warn on legitimate text files.
 */

import { fileExtension } from '#lib/utils/file-extension.js'

const IMAGE_EXTS = new Set([
  'jpg',
  'jpeg',
  'png',
  'gif',
  'webp',
  'heic',
  'heif',
  'bmp',
  'tiff',
  'tif',
  'ico',
  'icns',
  'avif',
  'raw',
  'cr2',
  'nef',
  'dng',
  'arw',
])

const DOCUMENT_EXTS = new Set([
  'pdf',
  'doc',
  'docx',
  'xls',
  'xlsx',
  'ppt',
  'pptx',
  'pages',
  'numbers',
  'key',
  'odt',
  'ods',
  'odp',
  'epub',
  'mobi',
])

const OTHER_BINARY_EXTS = new Set([
  // video
  'mp4',
  'mov',
  'avi',
  'mkv',
  'webm',
  'm4v',
  'mpg',
  'mpeg',
  'flv',
  'wmv',
  '3gp',
  // audio
  'mp3',
  'wav',
  'm4a',
  'aac',
  'ogg',
  'flac',
  'opus',
  'wma',
  // archive
  'zip',
  'tar',
  'tar.gz',
  'tar.bz2',
  'tar.xz',
  'tar.zst',
  'gz',
  'tgz',
  'bz2',
  'tbz',
  '7z',
  'rar',
  'dmg',
  'iso',
  'xz',
  'lz',
  'lzma',
  'jar',
  'war',
  // executable / binary
  'exe',
  'app',
  'msi',
  'pkg',
  'deb',
  'rpm',
  'dll',
  'so',
  'dylib',
  'bin',
  'dat',
  'o',
  'a',
  'lib',
  'class',
  'pyc',
  'pyo',
  // fonts
  'ttf',
  'otf',
  'woff',
  'woff2',
  'eot',
])

/**
 * The structural result of classifying a file for the raw-view banner.
 * `shouldWarn: false` means the file looks like something the raw view can
 * plausibly show (text, source code, no extension, etc.); for the rare
 * in-between case (SVG, JSON, CSV), the answer is also "don't warn" because the
 * raw bytes ARE useful there.
 *
 * The words the banner shows are NOT here: `category` and `ext` go to the
 * `viewer.binaryWarning.body` message, whose ICU select picks the phrase, so a
 * language with grammatical gender can make the sentence agree with it. Keep
 * this function pure and locale-free so it stays trivially testable.
 */
export interface ViewerWarning {
  shouldWarn: boolean
  /** The kind bucket the file falls in. `null` when `shouldWarn` is false. */
  category: 'image' | 'document' | 'binary' | null
  /** Uppercased extension (e.g. "ZIP"), only set for the `binary` category. */
  ext: string
}

export function categorizeForViewerWarning(fileName: string): ViewerWarning {
  // The viewer only opens files. The product's one rule, so `README`, `.bashrc`,
  // and `name.` have none, and `src.tar.gz` has `tar.gz`.
  const ext = fileExtension(fileName, 'file').toLowerCase()
  if (!ext) return { shouldWarn: false, category: null, ext: '' }
  if (IMAGE_EXTS.has(ext)) return { shouldWarn: true, category: 'image', ext: '' }
  if (DOCUMENT_EXTS.has(ext)) return { shouldWarn: true, category: 'document', ext: '' }
  if (OTHER_BINARY_EXTS.has(ext)) return { shouldWarn: true, category: 'binary', ext: ext.toUpperCase() }
  return { shouldWarn: false, category: null, ext: '' }
}
