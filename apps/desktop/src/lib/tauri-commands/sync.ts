// Synchronize folders (⇧F12): saved profiles, the options last used per pair of roots, and the check of the
// versions folder Settings names. The work is the backend's (`src-tauri/src/sync/`, `crates/cmdr-sync`); these
// are pass-throughs.

import {
  commands,
  type KeepPolicy,
  type KeepRefusal,
  type ProfileEditRefusal,
  type ProfileEntry,
  type ProfileMove,
  type ProfileSaveRefusal,
  type RootRef,
  type SyncOptions,
  type SyncProfile,
} from '#lib/ipc/bindings.js'
import { TypedFailure, failureOf } from '#lib/ipc/typed-failure.js'

export type {
  KeepPolicy,
  KeepRefusal,
  ProfileEditRefusal,
  ProfileEntry,
  ProfileMove,
  ProfileSaveRefusal,
  RootRef,
  SyncOptions,
  SyncProfile,
}

/** An `Error` that still carries the backend's typed profile-save refusal. */
export class SyncProfileSaveFailure extends TypedFailure<ProfileSaveRefusal> {
  constructor(failure: ProfileSaveRefusal) {
    super(failure, `sync profile save refused: ${failure.type}`)
    this.name = 'SyncProfileSaveFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asProfileSaveRefusal(error: unknown): ProfileSaveRefusal | null {
  return failureOf(SyncProfileSaveFailure, error)
}

/** An `Error` that still carries the backend's typed refusal of a rename, delete, move, or remembered-options write. */
export class SyncProfileEditFailure extends TypedFailure<ProfileEditRefusal> {
  constructor(failure: ProfileEditRefusal) {
    super(failure, `sync profile edit refused: ${failure.type}`)
    this.name = 'SyncProfileEditFailure'
  }
}

/** The typed refusal behind a caught value, or `null` when it isn't one. */
export function asProfileEditRefusal(error: unknown): ProfileEditRefusal | null {
  return failureOf(SyncProfileEditFailure, error)
}

function editResult<T>(res: { status: 'ok'; data: T } | { status: 'error'; error: ProfileEditRefusal }): T {
  if (res.status === 'error') throw new SyncProfileEditFailure(res.error)
  return res.data
}

/** The saved profiles, top to bottom. One that can't be used is listed with why, never dropped. */
export async function getSyncProfiles(): Promise<ProfileEntry[]> {
  return commands.getSyncProfiles()
}

/**
 * Saves a profile at the end of the list, or over the one with its name where that stands. Answers the list after.
 * Throws a {@link SyncProfileSaveFailure} (`listFull`, `invalid`) when nothing was saved.
 */
export async function saveSyncProfile(profile: SyncProfile): Promise<ProfileEntry[]> {
  const res = await commands.saveSyncProfile(profile)
  if (res.status === 'error') throw new SyncProfileSaveFailure(res.error)
  return res.data
}

/** Deletes a profile. Answers the list after; throws a {@link SyncProfileEditFailure} when the file couldn't be written. */
export async function deleteSyncProfile(id: string): Promise<ProfileEntry[]> {
  return editResult(await commands.deleteSyncProfile(id))
}

/**
 * Renames a profile in place. Throws a {@link SyncProfileEditFailure} (`nameTaken` when another profile has the name:
 * ask, then save over it; `writeFailed`). Answers the list after.
 */
export async function renameSyncProfile(id: string, name: string): Promise<ProfileEntry[]> {
  return editResult(await commands.renameSyncProfile(id, name))
}

/** Move up / Move down: swaps a profile with its neighbor. Answers the list after. */
export async function moveSyncProfile(id: string, direction: ProfileMove): Promise<ProfileEntry[]> {
  return editResult(await commands.moveSyncProfile(id, direction))
}

/** The options the dialog last ran with for this ordered pair of roots, if it remembers any. */
export async function getSyncLastOptions(left: RootRef, right: RootRef): Promise<SyncOptions | null> {
  return commands.getSyncLastOptions(left, right)
}

/** Remembers the dialog's options for this pair of roots. */
export async function saveSyncLastOptions(left: RootRef, right: RootRef, options: SyncOptions): Promise<void> {
  editResult(await commands.saveSyncLastOptions(left, right, options))
}

/** Why the versions folder `policy` names can't be used, or `null` when it can. */
export async function checkSyncKeepPolicy(policy: KeepPolicy): Promise<KeepRefusal | null> {
  return commands.checkSyncKeepPolicy(policy)
}
