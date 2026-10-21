// Unlock commands: hand a password to a locked container (an encrypted archive,
// a restic backup repo), cancel a check in flight, or forget the password.
//
// An archive only STORES the password (on the resolved `ArchiveVolume`,
// remember-for-this-archive, LRU-capped, forgotten on restart or eviction) and
// proves it on the retry. A repo CHECKS it before answering `unlocked` or
// `wrongPassword`, which takes a second or more, so the dialog names each check
// with an id it can cancel by.
//
// Backend: the container-neutral unlock seam,
// `apps/desktop/src-tauri/src/commands/file_system/unlock.rs`.

import {
  commands,
  type ArchivePasswordPrompt,
  type PasswordForgotten,
  type UnlockError,
  type UnlockOutcome,
} from '#lib/ipc/bindings.js'
import { TypedFailure } from '#lib/ipc/typed-failure.js'

export type { ArchivePasswordPrompt, PasswordForgotten, UnlockError, UnlockOutcome }

/** An unlock or a forget that didn't reach a container, with the backend's typed reason. */
export class UnlockFailure extends TypedFailure<UnlockError> {
  constructor(failure: UnlockError) {
    super(failure, failure.type)
    this.name = 'UnlockFailure'
  }
}

/**
 * Tries `password` on the container at `path` (the container itself or any
 * path inside it). Answers the container's typed outcome; throws an
 * {@link UnlockFailure} when it couldn't be asked (gone, timed out, cancelled
 * by {@link cancelContainerUnlock}). ❌ The password goes nowhere else.
 */
export async function unlockContainer(request: {
  parentVolumeId: string
  path: string
  password: string
  /** Save to the Keychain after a successful check. A container that keeps nothing (an archive) ignores it. */
  remember: boolean
  /** The caller's name for this check, for {@link cancelContainerUnlock}. */
  unlockId: string
}): Promise<UnlockOutcome> {
  const { parentVolumeId, path, password, remember, unlockId } = request
  const res = await commands.unlockContainer(parentVolumeId, path, password, remember, unlockId)
  if (res.status === 'error') throw new UnlockFailure(res.error)
  return res.data
}

/**
 * Drops the check named `unlockId`: it keeps nothing and saves nothing. Safe to
 * send before the check registers, or after it answered.
 */
export async function cancelContainerUnlock(unlockId: string): Promise<void> {
  await commands.cancelContainerUnlock(unlockId)
}

/**
 * Stores `password` for the archive at `archivePath` on `parentVolumeId`,
 * overwriting any previous one (so a fresh attempt replaces a rejected password).
 *
 * `archivePath` may be the archive file itself OR any path inside it — both
 * resolve to the same archive on the backend, so pass whichever is in hand (the
 * `.zip` path when prompting on a browse, an inner source path when prompting on
 * an extract).
 */
export async function setArchivePassword(parentVolumeId: string, archivePath: string, password: string): Promise<void> {
  // An archive answers at once and keeps nothing outside this session, so
  // there's nothing to cancel and nothing to remember.
  await unlockContainer({
    parentVolumeId,
    path: archivePath,
    password,
    remember: false,
    unlockId: crypto.randomUUID(),
  })
}

/**
 * Forgets the password the container holds in memory (the user cancelled the
 * prompt, or the frontend is resetting state). A no-op when it holds none.
 */
export async function forgetContainerPassword(parentVolumeId: string, path: string): Promise<void> {
  const res = await commands.forgetContainerPassword(parentVolumeId, path)
  if (res.status === 'error') throw new UnlockFailure(res.error)
}

/**
 * Forgets the container's password everywhere it's kept, as the person asked:
 * in memory, in the Keychain, and in anything opened with it, so the next read
 * asks again. Answers whether the Keychain let go of its copy; the container
 * is locked for this session either way.
 */
export async function forgetSavedContainerPassword(parentVolumeId: string, path: string): Promise<PasswordForgotten> {
  const res = await commands.forgetSavedContainerPassword(parentVolumeId, path)
  if (res.status === 'error') throw new UnlockFailure(res.error)
  return res.data
}

/**
 * Mirrors what the password prompt is ASKING to the backend, so `cmdr://state`
 * can name the container and the `unlock` tool can answer it. Without this
 * an agent reads a bare `- type: archive-password` and has nothing to act on.
 *
 * ❌ The password never rides this call: it carries the question, never the
 * answer.
 */
export async function notifyArchivePasswordPrompt(prompt: ArchivePasswordPrompt): Promise<void> {
  await commands.notifyArchivePasswordPrompt(prompt)
}

/** The prompt is gone (answered, cancelled, or swept). */
export async function notifyArchivePasswordDismissed(): Promise<void> {
  await commands.notifyArchivePasswordDismissed()
}
