/**
 * Spotting paths inside a restic repo's snapshot view.
 *
 * A pane shows a `Browse snapshots` folder inside a restic repo; everything
 * from that component down is served by the backend's restic volume, not the
 * disk. Lexical only, like the backend's `restic_route`
 * (`crates/cmdr-restic/src/path.rs`): the FIRST `Browse snapshots` component
 * marks the view, and the repo is the folder holding it. Keep the two in sync.
 *
 * A real folder of that name outside a repo matches too. The backend passes
 * such a folder through to the disk; the frontend can't tell without I/O, and
 * accepts the same edge the git portal's detector has.
 */

/** The view folder's name. A path component, so it stays English in every locale. */
export const RESTIC_VIEW_FOLDER_NAME = 'Browse snapshots'

const SEPARATOR = '/'

/** Index of the first view component in `path`'s components, or -1. */
function viewComponentIndex(components: string[]): number {
  return components.indexOf(RESTIC_VIEW_FOLDER_NAME)
}

/** Whether `path` runs through a restic repo's snapshot view. */
export function isVirtualResticPath(path: string): boolean {
  return viewComponentIndex(path.split(SEPARATOR)) !== -1
}

/** The repo folder `path` browses into, or `null` when it's outside every view. */
export function resticRepoRoot(path: string): string | null {
  const components = path.split(SEPARATOR)
  const index = viewComponentIndex(components)
  if (index === -1) return null
  return components.slice(0, index).join(SEPARATOR) || SEPARATOR
}
