/**
 * The file-name patterns the user last took in the Tags tab's From file name panel, newest first, at most ten,
 * remembered across sheets and restarts in `app-status.json` (`app-status-store.ts` validates them on load).
 */
import { TAG_NAME_PATTERNS_MAX, loadTagNamePatterns, saveTagNamePatterns } from '#lib/app-status-store.js'
import type { NamePattern } from '#lib/ipc/bindings.js'

export function loadNamePatterns(): Promise<NamePattern[]> {
  return loadTagNamePatterns()
}

/** `history` with `pattern` in front: a repeat (same mode and text) moves up instead of being listed twice. */
export function withPattern(history: readonly NamePattern[], pattern: NamePattern): NamePattern[] {
  const others = history.filter((entry) => entry.kind !== pattern.kind || entry.pattern !== pattern.pattern)
  return [pattern, ...others].slice(0, TAG_NAME_PATTERNS_MAX)
}

/** Remembers `pattern` after `history`; fire and forget. Returns the new list. */
export function rememberNamePattern(history: readonly NamePattern[], pattern: NamePattern): NamePattern[] {
  const next = withPattern(history, pattern)
  void saveTagNamePatterns(next)
  return next
}
