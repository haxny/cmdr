#!/usr/bin/env node
/**
 * `pnpm i18n:brief`: prints the translation brief for one batch of keys, the only
 * doc a translator agent needs to load up front. Assembly lives in
 * `i18n-brief-lib.ts`; this file parses flags, resolves the repo paths, and reads
 * git for `--changed-since` and for the English a stale translation was made from
 * (`i18n-brief-history.ts`).
 *
 *   --lang nl | nl,de | all          target locales (`all` = every full translation)
 *   --keys a.b.c,servers.sheet.*     exact keys or `*` globs
 *   --keys-file <path>               the same, one per line (`#` comments allowed)
 *   --missing                        English keys absent in any target locale
 *   --changed-since <git-ref>        English keys added or changed since this branch left the ref (merge base)
 *   --include-stale                  also every key a target translated from older English (widens the batch)
 *   --exclude-target-values          blind run: withhold the batch's current translations and decisions
 *   --parallel                       several translators at once: shared-file writes go to proposals
 *   --out <file>                     write there instead of stdout
 *   --stats                          per-section size to stderr
 *   --messages-root <dir>, --docs-root <dir>   fixtures
 *
 * Key selectors narrow each other: `--missing --keys servers.*` is the missing
 * keys under `servers.`. Schemas and the process: `docs/i18n/termbase.md`,
 * `docs/guides/i18n-translation.md`.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { parseArgs } from 'node:util'
import {
  BASE_LOCALE,
  listLocales,
  loadCatalog,
  mergeCatalogFiles,
  readLocaleFiles,
  resolveLocaleSource,
  resolveMessagesRoot,
} from './i18n-catalog-lib.ts'
import { EXIT_ERROR } from './i18n-locale-check-lib.ts'
import { buildBrief, renderBrief, roughTokens } from './i18n-brief-lib.ts'
import { changedKeys, selectKeys, staleKeys } from './i18n-brief-select.ts'
import { previousEnglish } from './i18n-brief-history.ts'

/** The parsed flags. */
export interface BriefArgs {
  lang?: string
  keys?: string
  keysFile?: string
  missing: boolean
  changedSince?: string
  includeStale: boolean
  excludeTargetValues: boolean
  parallel: boolean
  out?: string
  stats: boolean
  messagesRoot?: string
  docsRoot?: string
}

/**
 * Parses the flags strictly: an unknown flag or a stray argument is an error, so
 * `--keys a b c` (space-separated) fails loudly instead of briefing only `a`.
 * @param argv `process.argv.slice(2)`
 */
export function parseBriefArgs(argv: string[]): BriefArgs {
  const { values, positionals } = parseArgs({
    args: argv,
    options: {
      lang: { type: 'string' },
      keys: { type: 'string' },
      'keys-file': { type: 'string' },
      missing: { type: 'boolean', default: false },
      'changed-since': { type: 'string' },
      'include-stale': { type: 'boolean', default: false },
      'exclude-target-values': { type: 'boolean', default: false },
      parallel: { type: 'boolean', default: false },
      out: { type: 'string' },
      stats: { type: 'boolean', default: false },
      'messages-root': { type: 'string' },
      'docs-root': { type: 'string' },
    },
    allowPositionals: true,
  })
  if (positionals.length > 0) {
    throw new Error(
      `Unexpected argument(s): ${positionals.join(' ')}. Separate keys with commas (--keys a.b,c.d) or use --keys-file.`,
    )
  }
  return {
    lang: values.lang,
    keys: values.keys,
    keysFile: values['keys-file'],
    missing: values.missing,
    changedSince: values['changed-since'],
    includeStale: values['include-stale'],
    excludeTargetValues: values['exclude-target-values'],
    parallel: values.parallel,
    out: values.out,
    stats: values.stats,
    messagesRoot: values['messages-root'],
    docsRoot: values['docs-root'],
  }
}

/**
 * The commit `--changed-since <ref>` diffs against: where this branch left the
 * ref (`git merge-base <ref> HEAD`), not the ref's tip. Diffing against the tip
 * would also brief every key the ref changed after the split, which this branch
 * never touched.
 * @param runGit runs git with these args, returning trimmed stdout or `undefined` on failure
 */
export function changeBase(ref: string, runGit: (args: string[]) => string | undefined): string {
  const base = runGit(['merge-base', ref, 'HEAD'])
  if (!base) throw new Error(`--changed-since: "${ref}" isn't a git ref here, or shares no history with HEAD`)
  return base
}

/** Runs git in `cwd`, returning trimmed stdout, or `undefined` when git fails. */
function git(cwd: string, args: string[]): string | undefined {
  try {
    return execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()
  } catch {
    return undefined
  }
}

/** The worktree's root, and the MAIN clone's root (where the gitignored reference pile lives). */
function repoRoots(): { repoRoot: string; mainClone: string } {
  const fallback = join(import.meta.dirname, '..', '..', '..')
  const repoRoot = git(import.meta.dirname, ['rev-parse', '--show-toplevel']) ?? fallback
  const common = git(import.meta.dirname, ['rev-parse', '--path-format=absolute', '--git-common-dir'])
  return { repoRoot, mainClone: common ? dirname(common) : repoRoot }
}

/** Every locale the brief can target: full translations, not overlays or the base. */
function fullTranslations(messagesRoot?: string): string[] {
  const available = listLocales(messagesRoot)
  return available.filter((tag) => tag !== BASE_LOCALE && !resolveLocaleSource(tag, available).isOverlay)
}

/**
 * The English messages where this branch left a git ref (`changeBase`). A catalog
 * file that didn't exist then contributes nothing.
 */
function englishAt(ref: string, messagesRoot?: string): Record<string, string> {
  const enDir = join(resolveMessagesRoot(messagesRoot), BASE_LOCALE)
  const base = changeBase(ref, (args) => git(enDir, args))
  const files: Record<string, Record<string, unknown>> = {}
  for (const name of Object.keys(readLocaleFiles(BASE_LOCALE, messagesRoot))) {
    const text = git(enDir, ['show', `${base}:./${name}`])
    if (text !== undefined) files[name] = JSON.parse(text) as Record<string, unknown>
  }
  return mergeCatalogFiles(files).messages
}

/** The `--lang` locales, each checked to be a full translation. */
function resolveLangs(langFlag: string | undefined, messagesRoot?: string): string[] {
  if (!langFlag) throw new Error('Pass --lang <tag>[,<tag>…] or --lang all')
  const full = fullTranslations(messagesRoot)
  const langs = langFlag === 'all' ? full : langFlag.split(',').map((tag) => tag.trim())
  const unknown = langs.filter((tag) => !full.includes(tag))
  if (unknown.length > 0) throw new Error(`Not a full translation: ${unknown.join(', ')} (have: ${full.join(', ')})`)
  return langs
}

/**
 * The `--keys` patterns (comma-separated) plus the `--keys-file` ones (one per
 * line; blank lines and `#` comments skipped), or `undefined` when neither was passed.
 */
function keyPatterns(args: BriefArgs): string[] | undefined {
  const inline = args.keys?.split(',') ?? []
  const fromFile = args.keysFile ? readFileSync(args.keysFile, 'utf8').split('\n') : []
  const patterns = [...inline, ...fromFile].map((p) => p.trim()).filter((p) => p.length > 0 && !p.startsWith('#'))
  return patterns.length > 0 ? patterns : undefined
}

/**
 * The batch, from whichever key selectors were passed, plus how to describe it in
 * the header. `--include-stale` widens it (a union, unlike the narrowing selectors)
 * with every key a target translated from older English.
 */
function resolveKeys(args: BriefArgs, langs: readonly string[]) {
  const { messagesRoot, missing, changedSince: changedRef, includeStale } = args
  const patterns = keyPatterns(args)
  if (!patterns && !changedRef && !missing && !includeStale)
    throw new Error('Pick keys: --keys, --keys-file, --missing, --changed-since <ref>, and/or --include-stale')
  const en = loadCatalog(BASE_LOCALE, messagesRoot).messages
  const selected =
    patterns || changedRef || missing
      ? selectKeys({
          en,
          patterns,
          missingIn: missing ? langs.map((tag) => loadCatalog(tag, messagesRoot).messages) : undefined,
          changed: changedRef ? new Set(changedKeys(en, englishAt(changedRef, messagesRoot))) : undefined,
        })
      : []
  const stale = includeStale
    ? new Set(
        staleKeys(
          en,
          langs.map((tag) => loadCatalog(tag, messagesRoot)),
        ),
      )
    : new Set()
  const keys = Object.keys(en).filter((key) => selected.includes(key) || stale.has(key))
  return { keys, selection: selectionLabel(args, patterns) }
}

/** How the batch was picked, for the header. A long pattern list is already spelled out in the keys section. */
function selectionLabel(args: BriefArgs, patterns: readonly string[] | undefined): string {
  const keysPart =
    patterns && (patterns.length <= 3 ? `--keys ${patterns.join(',')}` : `--keys (${String(patterns.length)} patterns)`)
  return [
    keysPart,
    args.missing && '--missing',
    args.changedSince && `--changed-since ${args.changedSince}`,
    args.includeStale && '--include-stale',
  ]
    .filter(Boolean)
    .join(' ')
}

/** One `--stats` line. */
function statsLine(name: string, text: string): string {
  return `${name.padEnd(12)} ${String(text.length).padStart(7)} chars  ~${String(roughTokens(text)).padStart(6)} tokens`
}

function main(): void {
  const args = parseBriefArgs(process.argv.slice(2))
  const { messagesRoot } = args
  const langs = resolveLangs(args.lang, messagesRoot)
  const { keys, selection } = resolveKeys(args, langs)
  if (keys.length === 0) {
    console.error('No keys selected: nothing to brief.')
    return
  }

  const { repoRoot, mainClone } = repoRoots()
  const brief = buildBrief({
    langs,
    keys,
    selection,
    messagesRoot,
    docsRoot: args.docsRoot,
    pileRoot: join(mainClone, '_ignored', 'i18n'),
    repoRoot,
    excludeTargetValues: args.excludeTargetValues,
    parallel: args.parallel,
    previousEnglish: previousEnglish({ keys, langs, messagesRoot }),
  })
  const text = renderBrief(brief)

  if (args.out) writeFileSync(args.out, text)
  else process.stdout.write(text)

  if (args.stats) {
    for (const section of brief.sections) console.error(statsLine(section.name, section.text))
    console.error(statsLine('total', text))
  }
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    main()
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
