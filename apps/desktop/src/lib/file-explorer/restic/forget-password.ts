/**
 * "Forget backup password" (`restic.forgetPassword`): takes a restic repo's
 * remembered password out of the Keychain and locks the repo again, so the next
 * visit asks for it.
 *
 * The repo is the one the focused pane is inside, else the one whose
 * `Browse snapshots` row is under the cursor. The backend drops the password,
 * the opened repo, and its index too, so a pane left inside a snapshot would
 * only fail its next read: every pane inside the repo's view walks back out to
 * the repo folder, its cursor on `Browse snapshots`, ready to unlock again.
 */

import { forgetSavedContainerPassword, UnlockFailure } from '#lib/tauri-commands/index.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import type { NavigateIntent, NavigateResult } from '../pane/navigate'
import { RESTIC_VIEW_FOLDER_NAME, resticRepoRoot } from './path-detection'

const log = getAppLogger('fileExplorer')

type PaneId = 'left' | 'right'
const PANES: readonly PaneId[] = ['left', 'right']

/** The slice of the explorer this needs, so a test drives it with a fake. */
export interface ForgetPasswordExplorer {
  getFocusedPane: () => PaneId
  getPaneLocation: (pane: PaneId) => { volumeId: string; path: string }
  getFileAndPathUnderCursor: () => Promise<{ path: string; filename: string } | null>
  navigate: (intent: NavigateIntent) => NavigateResult
}

interface TargetRepo {
  /** The volume the repo lives on: a routed pane keeps its parent drive's id. */
  volumeId: string
  /** The repo folder. */
  root: string
}

/** The repo the person means, or `null` when they're near none. */
async function targetRepo(explorer: ForgetPasswordExplorer): Promise<TargetRepo | null> {
  const here = explorer.getPaneLocation(explorer.getFocusedPane())
  const inside = resticRepoRoot(here.path)
  if (inside !== null) return { volumeId: here.volumeId, root: inside }
  const underCursor = await explorer.getFileAndPathUnderCursor()
  const onRow = underCursor ? resticRepoRoot(underCursor.path) : null
  return onRow === null ? null : { volumeId: here.volumeId, root: onRow }
}

function viewOf(root: string): string {
  return root === '/' ? `/${RESTIC_VIEW_FOLDER_NAME}` : `${root}/${RESTIC_VIEW_FOLDER_NAME}`
}

function repoName(root: string): string {
  return root.split('/').filter(Boolean).at(-1) ?? root
}

/** Walks every pane inside `repo`'s view back out to the repo folder. */
function leaveTheView(explorer: ForgetPasswordExplorer, repo: TargetRepo): void {
  const focused = explorer.getFocusedPane()
  for (const pane of PANES) {
    const at = explorer.getPaneLocation(pane)
    if (at.volumeId !== repo.volumeId || resticRepoRoot(at.path) !== repo.root) continue
    explorer.navigate({
      pane,
      to: { goTo: { volumeId: repo.volumeId, path: repo.root } },
      // Only the pane the person is looking at may take focus.
      source: pane === focused ? 'user' : 'fallback',
      selectName: RESTIC_VIEW_FOLDER_NAME,
    })
  }
}

/** Forgets the target repo's saved password, locks it, and says how it went. */
export async function forgetResticPassword(explorer: ForgetPasswordExplorer): Promise<void> {
  const repo = await targetRepo(explorer)
  if (!repo) {
    addToast(tString('fileExplorer.restic.forgetPassword.noBackup'), { level: 'info' })
    return
  }
  const name = repoName(repo.root)
  let outcome
  try {
    outcome = await forgetSavedContainerPassword(repo.volumeId, viewOf(repo.root))
  } catch (err) {
    log.warn('Couldn’t forget a backup’s password ({reason}): {path}', {
      reason: failureOf(UnlockFailure, err)?.type ?? 'unexpected',
      path: repo.root,
    })
    addToast(tString('fileExplorer.restic.forgetPassword.unreachable', { name }), { level: 'warn' })
    return
  }
  leaveTheView(explorer, repo)
  if (outcome === 'everywhere') {
    addToast(tString('fileExplorer.restic.forgetPassword.forgotten', { name }), { level: 'success' })
  } else {
    addToast(tString('fileExplorer.restic.forgetPassword.keychainKeptIt', { name }), { level: 'warn' })
  }
}
