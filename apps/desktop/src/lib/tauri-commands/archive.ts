// Archive-password commands: store or clear the per-archive password used to
// unlock an encrypted archive (ZipCrypto today). The password lives on the
// resolved `ArchiveVolume` on the backend (remember-for-this-archive, LRU-capped,
// forgotten on restart or eviction). The frontend calls `setArchivePassword`
// after prompting, then retries the copy/move so the extract path can decrypt.
//
// Backend: the container-neutral unlock seam,
// `apps/desktop/src-tauri/src/commands/file_system/unlock.rs`. An archive only
// stores the password, so its `stored` outcome needs no handling here.

import { commands, type ArchivePasswordPrompt } from '#lib/ipc/bindings.js'

export type { ArchivePasswordPrompt }

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
  // An archive keeps nothing outside this session, so `remember` is moot.
  const res = await commands.unlockContainer(parentVolumeId, archivePath, password, false)
  if (res.status === 'error') throw new Error(res.error.type)
}

/**
 * Forgets any stored password for the archive (the user cancelled the prompt, or
 * the frontend is resetting state). A no-op when nothing was stored.
 */
export async function clearArchivePassword(parentVolumeId: string, archivePath: string): Promise<void> {
  const res = await commands.forgetContainerPassword(parentVolumeId, archivePath)
  if (res.status === 'error') throw new Error(res.error.type)
}

/**
 * Mirrors what the password prompt is ASKING to the backend, so `cmdr://state`
 * can name the archive and the `unlock` tool can answer it. Without this
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
