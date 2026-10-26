/**
 * Which keys a translation brief covers: the batch selectors `pnpm i18n:brief`
 * combines (`--keys`, `--missing`, `--changed-since`, `--include-stale`). Pure:
 * the CLI (`i18n-brief.ts`) reads the catalogs and git, this decides. Assembly of
 * the brief itself is `i18n-brief-lib.ts`.
 */

import { sourceHash } from './i18n-catalog-lib.ts'
import type { Catalog } from './i18n-catalog-lib.ts'

/** Glob (`*` only) to an anchored regex over keys. */
function keyGlob(pattern: string): RegExp {
  const body = pattern
    .split('*')
    .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
    .join('.*')
  return new RegExp(`^${body}$`)
}

/**
 * Picks the batch. Every given selector narrows it: `patterns` (exact keys or `*`
 * globs), `missingIn` (keys absent from ANY of these locale catalogs), `changed`
 * (keys added or edited since a ref). Catalog order is kept.
 *
 * @throws when a pattern matches no English key, since a typo would otherwise
 *   produce a silently smaller batch
 */
export function selectKeys({
  en,
  patterns,
  missingIn,
  changed,
}: {
  en: Record<string, string>
  patterns?: readonly string[]
  missingIn?: readonly Record<string, string>[]
  changed?: ReadonlySet<string>
}): string[] {
  let keys = Object.keys(en)
  if (patterns && patterns.length > 0) {
    const globs = patterns.map((pattern) => ({ pattern, re: keyGlob(pattern) }))
    const dead = globs.filter(({ re }) => !keys.some((key) => re.test(key))).map(({ pattern }) => pattern)
    if (dead.length > 0) throw new Error(`No English key matches: ${dead.join(', ')}`)
    keys = keys.filter((key) => globs.some(({ re }) => re.test(key)))
  }
  if (missingIn) keys = keys.filter((key) => missingIn.some((locale) => !(key in locale)))
  if (changed) keys = keys.filter((key) => changed.has(key))
  return keys
}

/**
 * Keys any of `targets` translated from other English (its `@key.sourceHash` names
 * another value) or carries with no hash at all, in English catalog order: the
 * stale check's definition, so the brief and the check agree.
 */
export function staleKeys(en: Record<string, string>, targets: readonly Catalog[]): string[] {
  return Object.keys(en).filter((key) =>
    targets.some((target) => {
      if (!(key in target.messages)) return false
      const stored = (target.metadata[key] as Record<string, unknown> | undefined)?.sourceHash
      return stored !== sourceHash(en[key])
    }),
  )
}

/** Keys added, or whose value changed, between `previous` and `current`, in current order. */
export function changedKeys(current: Record<string, string>, previous: Record<string, string>): string[] {
  return Object.keys(current).filter((key) => previous[key] !== current[key])
}
