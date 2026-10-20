/**
 * Shared helper for spotting paths inside the virtual `.git` portal.
 *
 * The backend exposes six categories under `.git/`: `branches`, `tags`,
 * `commits`, `stash`, `worktrees`, and `submodules`. Subpaths that route
 * through any of these are virtual (they don't exist on disk in the shape
 * Cmdr presents them). The `.git` directory itself (and other real `.git/`
 * internals like `HEAD` or `refs/heads/main`) stay real.
 *
 * The frontend uses this to skip filesystem-bound bookkeeping (the
 * "directory still exists" poll, future similar checks) on virtual paths.
 *
 * The six category names mirror `Cat` in `crates/cmdr-git/src/path.rs`.
 * Keep them in sync.
 */

/** The six category folders the portal adds under `.git/`. */
export const VIRTUAL_GIT_CATEGORIES = ['branches', 'tags', 'commits', 'stash', 'worktrees', 'submodules'] as const

const VIRTUAL_GIT_PATH_REGEX = new RegExp(`/\\.git/(?:${VIRTUAL_GIT_CATEGORIES.join('|')})(?:/|$)`)

/**
 * Returns `true` when `path` lives under one of the six virtual git
 * categories. Returns `false` for the `.git` root itself, for raw git
 * internals like `.git/HEAD` or `.git/refs/heads/main`, and for normal
 * non-git paths.
 */
export function isVirtualGitPath(path: string): boolean {
  return VIRTUAL_GIT_PATH_REGEX.test(path)
}

const CATEGORY_NAMES: ReadonlySet<string> = new Set(VIRTUAL_GIT_CATEGORIES)

/**
 * Index of the `.git` component that opens a virtual category in a path's
 * `/`-split components, or -1. The same test as `isVirtualGitPath`, per
 * component, so a caller can order the portal against other routed boundaries
 * (the leftmost one wins, as in the backend's `resolve`).
 */
export function virtualGitBoundaryIndex(components: readonly string[]): number {
  for (let i = 1; i + 1 < components.length; i++) {
    if (components[i] === '.git' && CATEGORY_NAMES.has(components[i + 1])) return i
  }
  return -1
}
