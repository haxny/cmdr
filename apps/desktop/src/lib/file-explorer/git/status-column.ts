/**
 * Helpers for the optional Git status column in Full mode.
 *
 * Each entry has a single-glyph code with a long-form `aria-label` and tooltip.
 * Backend ships `EntryStatus` rows from `get_git_status_for_paths`; the
 * `byPath` map indexes them by relative path so `FullList.svelte` can render
 * the cell in O(1).
 */
import { getGitStatusForPaths } from '#lib/tauri-commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'

export type EntryStatusCode =
  | 'modified'
  | 'added'
  | 'deleted'
  | 'renamed'
  | 'copied'
  | 'typechange'
  | 'untracked'
  | 'ignored'
  | 'conflicted'

const codeToGlyph: Record<EntryStatusCode, string> = {
  modified: 'M',
  added: 'A',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
  typechange: 'T',
  untracked: '?',
  ignored: '!',
  conflicted: 'U',
}

const codeToLabelKey: Record<EntryStatusCode, MessageKey> = {
  modified: 'fileExplorer.git.status.modified',
  added: 'fileExplorer.git.status.added',
  deleted: 'fileExplorer.git.status.deleted',
  renamed: 'fileExplorer.git.status.renamed',
  copied: 'fileExplorer.git.status.copied',
  typechange: 'fileExplorer.git.status.typechange',
  untracked: 'fileExplorer.git.status.untracked',
  ignored: 'fileExplorer.git.status.ignored',
  conflicted: 'fileExplorer.git.status.conflicted',
}

export function glyphFor(code: EntryStatusCode): string {
  return codeToGlyph[code]
}

/** The status in the UI language: the Git cell's tooltip and `aria-label`, and the row name's "{status} in git". */
export function labelFor(code: EntryStatusCode): string {
  return tString(codeToLabelKey[code])
}

/**
 * Fetches a per-path status map for a directory inside a worktree.
 * Returns `null` if the lookup timed out so callers can render a placeholder.
 */
export async function fetchStatusMap(repoRoot: string, dir: string): Promise<Map<string, EntryStatusCode> | null> {
  const result = await getGitStatusForPaths(repoRoot, dir)
  if (result.timedOut) return null
  const map = new Map<string, EntryStatusCode>()
  for (const entry of result.data) {
    // For renames the relative path is `old -> new`; key on the new side.
    const path = entry.relativePath.includes(' -> ') ? entry.relativePath.split(' -> ')[1] : entry.relativePath
    map.set(path, entry.code)
  }
  return map
}
