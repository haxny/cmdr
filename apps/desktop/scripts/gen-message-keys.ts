#!/usr/bin/env node
/**
 * Generates `src/lib/intl/keys.gen.ts`: the `MessageKey` union of every catalog
 * key under `src/lib/intl/messages/en/`, and fails (exit 1) on keys referenced
 * in code but ABSENT from the catalog, because `t('missing.key')` would
 * silently fall back to the raw key string at runtime.
 *
 * The inverse (catalog keys nobody references) is the `message-keys-unused` Go
 * check's job, not this script's: it matches every key in one pass per file.
 *
 * Pure logic lives in `gen-message-keys-lib.ts` (unit-tested); this CLI does the
 * filesystem I/O and the source scan. Run via `pnpm intl:keys` from the desktop
 * app dir, or through the `keys-fresh` check in the pipeline. Never hand-edit
 * `keys.gen.ts`.
 */

import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { collectCatalogKeys, extractUsedKeys, findMissingKeys, emitKeysModule } from './gen-message-keys-lib.ts'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
const messagesDir = join(desktopDir, 'src', 'lib', 'intl', 'messages', 'en')
const srcDir = join(desktopDir, 'src')
const outFile = join(desktopDir, 'src', 'lib', 'intl', 'keys.gen.ts')

/** Source extensions to scan for key references. */
const SCANNED_EXTS = new Set(['.ts', '.svelte'])
/** Directories to skip when walking for key usages. */
const SKIP_DIRS = new Set(['node_modules', 'target', '.svelte-kit'])
/**
 * Files to skip in the usage scan. The `#lib/intl` runtime files define and
 * document the accessors (`t('key')` appears in their JSDoc, not as a call
 * site), and test files deliberately reference unknown keys to exercise the
 * fallback. Counting either would produce phantom "missing key" failures.
 */
const SKIP_FILES = new Set(['messages.svelte.ts', 'Trans.svelte'])

/**
 * Whether a filename should be excluded from the usage scan.
 */
function isScanExcluded(name: string): boolean {
  return SKIP_FILES.has(name) || name.endsWith('.test.ts') || name.endsWith('.test.js')
}

/** Reads and parses every `en/*.json` catalog into a filename → JSON map. */
function readCatalogFiles(): Record<string, Record<string, unknown>> {
  const files: Record<string, Record<string, unknown>> = {}
  for (const file of readdirSync(messagesDir)) {
    if (!file.endsWith('.json')) continue
    files[file] = JSON.parse(readFileSync(join(messagesDir, file), 'utf8')) as Record<string, unknown>
  }
  return files
}

/**
 * Recursively collects the keys the frontend references via
 * `t()`/`tString()`/`getMessage()`/`<Trans>`/`*Key:` props into `acc`. The
 * generated `keys.gen.ts` is skipped (it lists the catalog keys, not usages).
 * Rust's `menu_t` call sites aren't read: the `MessageKey` union is a frontend
 * type, so missing-key detection is frontend-only.
 */
function scanUsedKeys(dir: string, acc: Set<string>): void {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) scanUsedKeys(join(dir, entry.name), acc)
      continue
    }
    if (!SCANNED_EXTS.has(extname(entry.name))) continue
    if (entry.name === 'keys.gen.ts' || isScanExcluded(entry.name)) continue
    for (const key of extractUsedKeys(readFileSync(join(dir, entry.name), 'utf8'))) acc.add(key)
  }
}

const catalogKeys = collectCatalogKeys(readCatalogFiles())
writeFileSync(outFile, emitKeysModule(catalogKeys))

const usedKeys = new Set<string>()
scanUsedKeys(srcDir, usedKeys)
const missing = findMissingKeys(catalogKeys, usedKeys)

console.log(`Wrote ${String(catalogKeys.length)} message keys to keys.gen.ts`)

if (missing.length > 0) {
  console.error(
    `\nFailure: ${String(missing.length)} key(s) referenced in code but missing from the catalog:\n` +
      missing.map((k) => `  - ${k}`).join('\n') +
      `\nAdd them to a \`messages/en/*.json\` file (the key prefix maps to the filename).`,
  )
  process.exit(1)
}
